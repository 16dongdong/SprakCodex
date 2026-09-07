import { expect, test } from "@playwright/test";

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
  upstreamStreamTimeoutMs: 600000,
  sseKeepaliveIntervalMs: 15000,
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

function usageSnapshot(availableCount: number) {
  return {
    accountId: "acct-reset-credit",
    availabilityStatus: "available",
    usedPercent: 92,
    windowMinutes: 300,
    resetsAt: 1_900_000_000,
    secondaryUsedPercent: 30,
    secondaryWindowMinutes: 10_080,
    secondaryResetsAt: 1_900_003_600,
    creditsJson: JSON.stringify({
      rate_limit_reset_credits: { available_count: availableCount },
    }),
    capturedAt: 100,
  };
}

test("account reset-credit control verifies, confirms, consumes, and refreshes usage", async ({
  page,
}) => {
  let consumed = false;
  let readCount = 0;
  let consumeCount = 0;
  let consumeParams: Record<string, unknown> = {};

  await page.route(/\/api\/runtime(?:\?.*)?$/, async (route) => {
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

  await page.route("**/api/rpc", async (route) => {
    const payload = route.request().postDataJSON();
    const method = typeof payload?.method === "string" ? payload.method : "";
    const id = payload?.id ?? 1;
    const ok = (result: unknown) =>
      route.fulfill({
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify({ jsonrpc: "2.0", id, result }),
      });

    if (method === "appSettings/get") {
      await ok(SETTINGS_SNAPSHOT);
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
        permissions: ["system:admin"],
        distributionEnabled: false,
      });
      return;
    }
    if (method === "account/list") {
      await ok({
        items: [
          {
            id: "acct-reset-credit",
            name: "reset-credit@example.com",
            label: "Reset Credit Account",
            plan_type: "plus",
            status: "active",
            sort: 0,
          },
          {
            id: "acct-without-credit-field",
            name: "without-credit@example.com",
            label: "No Reset Field",
            plan_type: "plus",
            status: "active",
            sort: 1,
          },
        ],
        total: 2,
        page: 1,
        pageSize: 20,
      });
      return;
    }
    if (method === "account/usage/list") {
      await ok({
        items: [
          usageSnapshot(consumed ? 1 : 2),
          {
            ...usageSnapshot(0),
            accountId: "acct-without-credit-field",
            creditsJson: JSON.stringify({ balance: 0 }),
          },
        ],
      });
      return;
    }
    if (method === "account/usage/resetCredits") {
      readCount += 1;
      await ok({
        availableCount: consumed ? 1 : 2,
        nextExpiresAt: 1_893_456_000,
        credits: [
          {
            id: "credit-1",
            status: "available",
            resetType: "five_hour",
            grantedAt: 1_800_000_000,
            expiresAt: 1_893_456_000,
          },
        ],
      });
      return;
    }
    if (method === "account/usage/resetCredit/consume") {
      consumeCount += 1;
      consumeParams =
        payload?.params && typeof payload.params === "object"
          ? (payload.params as Record<string, unknown>)
          : {};
      consumed = true;
      await ok({
        consumed: true,
        usageRefreshed: true,
        warning: null,
        snapshot: {
          availableCount: 1,
          nextExpiresAt: 1_893_456_000,
          credits: [],
        },
      });
      return;
    }

    await route.fulfill({
      status: 500,
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: { code: -32000, message: `Unhandled RPC method in test: ${method}` },
      }),
    });
  });

  await page.goto("/");
  await page.getByRole("link", { name: "OpenAI 账号池" }).click();
  await expect(page.getByRole("heading", { name: "OpenAI 账号池" })).toBeVisible();

  const resetButton = page.getByRole("button", {
    name: "重置 5 小时和 7 天额度，可用 2 次",
  });
  await expect(resetButton).toBeVisible();
  await expect(page.getByRole("button", { name: /重置 5 小时和 7 天额度/ })).toHaveCount(1);

  await resetButton.click();
  const dialog = page.getByRole("dialog", {
    name: "重置当前 5 小时和 7 天额度",
  });
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByText(
      "此操作会消耗 1 次重置券，并同时恢复当前 5 小时和 7 天额度。提交成功后无法撤销。",
    ),
  ).toBeVisible();
  await expect(dialog.getByText("Reset Credit Account")).toBeVisible();
  await expect(dialog.getByText("可用次数")).toBeVisible();
  await expect(dialog.getByText("2", { exact: true })).toBeVisible();
  await expect.poll(() => readCount).toBe(1);

  await dialog.getByRole("button", { name: "消耗 1 次并重置" }).click();
  await expect.poll(() => consumeCount).toBe(1);
  expect(consumeParams.accountId).toBe("acct-reset-credit");
  await expect(page.getByText("5 小时和 7 天额度已重置")).toBeVisible();
  await expect(
    page.getByRole("button", {
      name: "重置 5 小时和 7 天额度，可用 1 次",
    }),
  ).toBeVisible();
});
