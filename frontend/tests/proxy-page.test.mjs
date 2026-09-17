import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "../..");
const pageSource = await fs.readFile(
  path.join(repositoryRoot, "frontend/src/app/proxy/page.tsx"),
  "utf8",
);
const transportSource = await fs.readFile(
  path.join(repositoryRoot, "backend/crates/service/src/directObservation/transport.rs"),
  "utf8",
);
const settingsSource = await fs.readFile(
  path.join(repositoryRoot, "backend/crates/service/src/app_settings/shared.rs"),
  "utf8",
);

test("代理页用独立开关和地址共同提交配置", () => {
  assert.match(pageSource, /upstreamProxyEnabled: enabled/);
  assert.match(pageSource, /upstreamProxyUrl: normalizedUrl/);
  assert.match(pageSource, /<Switch[\s\S]*?onCheckedChange/);
  assert.match(pageSource, /observationClient\.stop\(\)[\s\S]*?observationClient\.start\(\)/);
});

test("启用用户代理后覆盖目标进程原系统代理", () => {
  assert.match(settingsSource, /gateway\.upstream_proxy_enabled/);
  assert.match(transportSource, /if engine\.configuredProxyEnabled \{[\s\S]*?serveConnection\(stream, engine, None\)/);
  assert.match(transportSource, /if engine\.configuredProxyEnabled \{[\s\S]*?engine\.connect\(&targetHost, target\.port\)/);
});
