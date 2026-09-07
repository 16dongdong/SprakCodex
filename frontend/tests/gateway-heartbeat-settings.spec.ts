import { expect, test, type Page } from "@playwright/test";

const SETTINGS_SNAPSHOT = {
  updateAutoCheck: true,
  closeToTrayOnClose: false,
  closeToTraySupported: false,
  lowTransparency: false,
  lightweightModeOnCloseToTray: false,
  codexCliGuideDismissed: true,
  webAccessPasswordConfigured: false,
  locale: "zh-CN",
  localeOptions: ["zh-CN", "en"],
  serviceAddr: "localhost:48760",
  serviceListenMode: "loopback",
  serviceListenModeOptions: ["loopback", "all_interfaces"],
  routeStrategy: "ordered",
  routeStrategyOptions: ["ordered", "balanced"],
  freeAccountMaxModel: "auto",
  freeAccountMaxModelOptions: ["auto", "gpt-5"],
  modelForwardRules: "",
  accountMaxInflight: 1,
  gatewayOriginator: "codex-cli",
  gatewayOriginatorDefault: "codex-cli",
  gatewayUserAgentVersion: "1.0.0",
  gatewayUserAgentVersionDefault: "1.0.0",
  gatewayResidencyRequirement: "",
  gatewayResidencyRequirementOptions: ["", "us"],
  pluginMarketMode: "builtin",
  pluginMarketSourceUrl: "",
  upstreamProxyUrl: "",
  upstreamStreamTimeoutMs: 600_000,
  upstreamTotalTimeoutMs: 0,
  sseKeepaliveIntervalMs: 15_000,
  backgroundTasks: {
    usagePollingEnabled: true,
    usagePollIntervalSecs: 600,
    gatewayKeepaliveEnabled: true,
    gatewayKeepaliveIntervalSecs: 180,
    tokenRefreshPollingEnabled: true,
    tokenRefreshPollIntervalSecs: 60,
    usageRefreshWorkers: 4,
    httpWorkerFactor: 4,
    httpWorkerMin: 8,
    httpStreamWorkerFactor: 1,
    httpStreamWorkerMin: 2,
  },
  envOverrides: {},
  envOverrideCatalog: [],
  envOverrideReservedKeys: [],
  envOverrideUnsupportedKeys: [],
  theme: "tech",
  appearancePreset: "classic",
};

async function mockRuntimeAndRpc(page: Page) {
  let settingsSnapshot = { ...SETTINGS_SNAPSHOT };
  const settingsPatches: Record<string, unknown>[] = [];

  await page.route(/\/api\/runtime\/?(?:\?.*)?$/, async (route) => {
    await route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        mode: "web-gateway",
        rpcBaseUrl: "/api/rpc",
        canManageService: false,
        canSelfUpdate: false,
        canCloseToTray: false,
        canOpenLocalDir: false,
        canUseBrowserFileImport: true,
        canUseBrowserDownloadExport: true,
      }),
    });
  });

  await page.route(/\/api\/rpc\/?(?:\?.*)?$/, async (route) => {
    const payload = route.request().postDataJSON();
    const method = typeof payload?.method === "string" ? payload.method : "";
    const id = payload?.id ?? 1;
    const params =
      payload?.params && typeof payload.params === "object"
        ? (payload.params as Record<string, unknown>)
        : {};
    const ok = (result: unknown) =>
      route.fulfill({
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify({ jsonrpc: "2.0", id, result }),
      });

    if (method === "appSettings/get") {
      await ok(settingsSnapshot);
      return;
    }
    if (method === "appSettings/set") {
      settingsPatches.push(params);
      settingsSnapshot = { ...settingsSnapshot, ...params };
      await ok(settingsSnapshot);
      return;
    }
    if (method === "initialize") {
      await ok({
        userAgent: "codex_cli_rs/0.1.19",
        codexHome: "C:/Users/Test/.codex",
        platformFamily: "windows",
        platformOs: "windows",
      });
      return;
    }
    if (method === "accountManager/session/current") {
      await ok({
        mode: "none",
        currentUser: null,
        role: "system_admin",
        permissions: [],
        distributionEnabled: false,
      });
      return;
    }
    if (method === "aggregateApi/list") {
      await ok({ items: [] });
      return;
    }
    if (method === "gateway/concurrencyRecommendation/get") {
      await ok({
        usageRefreshWorkers: 4,
        httpWorkerFactor: 4,
        httpWorkerMin: 8,
        httpStreamWorkerFactor: 1,
        httpStreamWorkerMin: 2,
        accountMaxInflight: 1,
      });
      return;
    }

    await route.fulfill({
      status: 500,
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: { code: -32000, message: `Unhandled RPC method: ${method}` },
      }),
    });
  });

  return settingsPatches;
}

test("gateway heartbeat switch persists and controls the interval input", async ({
  page,
}) => {
  const settingsPatches = await mockRuntimeAndRpc(page);

  await page.goto("/settings/");
  await page.getByRole("tab", { name: "网关" }).click();

  const heartbeatSwitch = page.getByRole("switch", { name: "保持连接心跳" });
  const intervalInput = page.getByLabel("SSE 保活间隔 (ms)");
  await expect(heartbeatSwitch).toBeChecked();
  await expect(intervalInput).toBeEnabled();

  await heartbeatSwitch.click();

  await expect.poll(() => settingsPatches.length).toBe(1);
  expect(settingsPatches[0]?.sseKeepaliveEnabled).toBe(false);
  await expect(heartbeatSwitch).not.toBeChecked();
  await expect(intervalInput).toBeDisabled();
});

test("Free account model ceiling persists the selected model", async ({ page }) => {
  const settingsPatches = await mockRuntimeAndRpc(page);

  await page.goto("/settings/");
  await page.getByRole("tab", { name: "网关" }).click();

  const ceilingSelect = page.getByRole("combobox", {
    name: "Free 账号模型上限",
  });
  await expect(ceilingSelect).toContainText("不限制");
  await ceilingSelect.click();
  await page.getByRole("option", { name: "gpt-5" }).click();

  await expect.poll(() => settingsPatches.length).toBe(1);
  expect(settingsPatches[0]?.freeAccountMaxModel).toBe("gpt-5");
  await expect(ceilingSelect).toContainText("gpt-5");
});
