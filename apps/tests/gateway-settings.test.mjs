import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "gateway-settings.ts"
);
const transportSettingsSourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "gateway",
  "transport-settings.ts"
);
const codexConstantsSourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "constants",
  "codex.ts"
);
const serviceRuntimeConfigSourcePath = path.resolve(
  appsRoot,
  "..",
  "crates",
  "service",
  "src",
  "gateway",
  "core",
  "runtime_config.rs"
);
const modelCatalogSeedPath = path.resolve(
  appsRoot,
  "..",
  "crates",
  "core",
  "seeds",
  "model_catalog_v2_2026_07_10.json"
);

async function compileModule(sourcePath) {
  const source = await fs.readFile(sourcePath, "utf8");
  return ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  }).outputText;
}

async function loadGatewaySettingsModule() {
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-gateway-settings-")
  );
  const tempFile = path.join(tempDir, "gateway-settings.mjs");
  const transportSettingsTempFile = path.join(
    tempDir,
    "transport-settings.mjs"
  );
  const [gatewaySettingsOutput, transportSettingsOutput] = await Promise.all([
    compileModule(sourcePath),
    compileModule(transportSettingsSourcePath),
  ]);
  await Promise.all([
    fs.writeFile(
      tempFile,
      gatewaySettingsOutput.replace(
        'from "@/lib/gateway/transport-settings"',
        'from "./transport-settings.mjs"'
      ),
      "utf8"
    ),
    fs.writeFile(transportSettingsTempFile, transportSettingsOutput, "utf8"),
  ]);
  return {
    gatewaySettings: await import(pathToFileURL(tempFile).href),
    transportSettings: await import(pathToFileURL(transportSettingsTempFile).href),
  };
}

const { gatewaySettings, transportSettings } =
  await loadGatewaySettingsModule();

function parseNumericVersion(value) {
  return value.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

function compareNumericVersions(left, right) {
  const leftParts = parseNumericVersion(left);
  const rightParts = parseNumericVersion(right);
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const difference = (leftParts[index] || 0) - (rightParts[index] || 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

test("Codex 默认 client_version 前后端一致且覆盖内置模型最低版本", async () => {
  const [frontendSource, backendSource, seedSource] = await Promise.all([
    fs.readFile(codexConstantsSourcePath, "utf8"),
    fs.readFile(serviceRuntimeConfigSourcePath, "utf8"),
    fs.readFile(modelCatalogSeedPath, "utf8"),
  ]);
  const frontendVersion = frontendSource.match(
    /DEFAULT_CODEX_USER_AGENT_VERSION\s*=\s*"([^"]+)"/,
  )?.[1];
  const backendVersion = backendSource.match(
    /DEFAULT_CODEX_USER_AGENT_VERSION:\s*&str\s*=\s*"([^"]+)"/,
  )?.[1];
  assert.ok(frontendVersion, "frontend Codex default version missing");
  assert.ok(backendVersion, "backend Codex default version missing");
  assert.equal(frontendVersion, backendVersion);

  const seed = JSON.parse(seedSource);
  const minimumVersions = seed.models
    .map((model) => model.capabilities?.minimal_client_version)
    .filter((version) => typeof version === "string" && version.length > 0);
  assert.ok(minimumVersions.length > 0, "builtin model minimum versions missing");
  const newestMinimum = minimumVersions.sort(compareNumericVersions).at(-1);
  assert.ok(
    compareNumericVersions(frontendVersion, newestMinimum) >= 0,
    `default client_version ${frontendVersion} is older than builtin model minimum ${newestMinimum}`,
  );
});

test("gateway transport values share defaults and normalization", () => {
  assert.deepEqual(transportSettings.DEFAULT_GATEWAY_TRANSPORT_VALUES, {
    sseKeepaliveEnabled: true,
    sseKeepaliveIntervalMs: 15_000,
    upstreamStreamTimeoutMs: 300_000,
    upstreamTotalTimeoutMs: 0,
  });
  assert.deepEqual(
    transportSettings.normalizeGatewayTransportValues({
      sse_keepalive_enabled: "off",
      sse_keepalive_interval_ms: "5000",
      upstream_stream_timeout_ms: "120000",
      upstream_total_timeout_ms: -1,
    }),
    {
      sseKeepaliveEnabled: false,
      sseKeepaliveIntervalMs: 5_000,
      upstreamStreamTimeoutMs: 120_000,
      upstreamTotalTimeoutMs: 0,
    }
  );
});

test("readGatewayTransportSettings 读取真实传输配置并补齐默认值", () => {
  const settings = gatewaySettings.readGatewayTransportSettings({
    sseKeepaliveEnabled: false,
    sseKeepaliveIntervalMs: 5000,
    upstreamStreamTimeoutMs: "120000",
  });

  assert.equal(settings.sseKeepaliveEnabled, false);
  assert.equal(settings.sseKeepaliveIntervalMs, 5000);
  assert.equal(settings.upstreamStreamTimeoutMs, 120000);
  assert.deepEqual(settings.envKeys, [
    "CODEXMANAGER_SSE_KEEPALIVE_ENABLED",
    "CODEXMANAGER_SSE_KEEPALIVE_INTERVAL_MS",
    "CODEXMANAGER_UPSTREAM_STREAM_TIMEOUT_MS",
    "CODEXMANAGER_UPSTREAM_TOTAL_TIMEOUT_MS",
  ]);
  assert.equal(settings.requiresRestart, false);

  const legacySettings = gatewaySettings.readGatewayTransportSettings({});
  assert.equal(legacySettings.sseKeepaliveEnabled, true);
});

test("readGatewayUpstreamProxySettings 与 readGatewayRouteStrategySettings 对齐对象返回", () => {
  const proxy = gatewaySettings.readGatewayUpstreamProxySettings({
    proxyUrl: "http://127.0.0.1:7890",
  });
  assert.equal(proxy.proxyUrl, "http://127.0.0.1:7890");
  assert.equal(proxy.envKey, "CODEXMANAGER_UPSTREAM_PROXY_URL");

  const route = gatewaySettings.readGatewayRouteStrategySettings({
    strategy: "balanced",
    manualPreferredAccountId: "acc-1",
  });
  assert.equal(route.strategy, "balanced");
  assert.deepEqual(route.options, ["ordered", "balanced"]);
  assert.equal(route.manualPreferredAccountId, "acc-1");
});

test("readGatewayConcurrencyRecommendation 解析推荐并补齐保底值", () => {
  const recommendation = gatewaySettings.readGatewayConcurrencyRecommendation({
    cpuCores: "12",
    memoryMib: 32768,
    usageRefreshWorkers: 6,
    httpWorkerFactor: "5",
    httpWorkerMin: 12,
    httpStreamWorkerFactor: 2,
    httpStreamWorkerMin: 4,
    accountMaxInflight: 2,
  });

  assert.equal(recommendation.cpuCores, 12);
  assert.equal(recommendation.memoryMib, 32768);
  assert.equal(recommendation.usageRefreshWorkers, 6);
  assert.equal(recommendation.httpWorkerFactor, 5);
  assert.equal(recommendation.httpWorkerMin, 12);
  assert.equal(recommendation.httpStreamWorkerFactor, 2);
  assert.equal(recommendation.httpStreamWorkerMin, 4);
  assert.equal(recommendation.accountMaxInflight, 2);
  assert.equal(recommendation.queueWaitTimeoutMs, 100);
});

test("readServiceListenConfig 对齐监听模式配置返回", () => {
  const listenConfig = gatewaySettings.readServiceListenConfig({
    mode: "all_interfaces",
    requiresRestart: true,
  });
  assert.equal(listenConfig.mode, "all_interfaces");
  assert.deepEqual(listenConfig.options, ["loopback", "all_interfaces"]);
  assert.equal(listenConfig.requiresRestart, true);
});

test("readGatewayManualAccountId 统一读取 manual account id", () => {
  assert.equal(
    gatewaySettings.readGatewayManualAccountId({ accountId: "acc-9" }),
    "acc-9"
  );
  assert.equal(gatewaySettings.readGatewayManualAccountId(null), "");
});
