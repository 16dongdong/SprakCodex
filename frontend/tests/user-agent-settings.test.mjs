import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

async function readSource(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

test("gateway settings expose one global User-Agent with the Codex fallback", async () => {
  const [types, constants, normalize, store, settingsPage, gatewayTab, aggregatePage] =
    await Promise.all([
      readSource("src/types/settings.ts"),
      readSource("src/lib/constants/codex.ts"),
      readSource("src/lib/api/normalize.ts"),
      readSource("src/lib/store/useAppStore.ts"),
      readSource("src/app/settings/page.tsx"),
      readSource("src/app/settings/components/gateway-tab-content.tsx"),
      readSource("src/app/aggregate-api/page.tsx"),
    ]);

  assert.match(types, /gatewayUserAgent:\s*string;/);
  assert.match(types, /gatewayUserAgentDefault:\s*string;/);
  assert.doesNotMatch(types, /aggregateApiProbeUserAgent/);
  assert.match(
    constants,
    /DEFAULT_CODEX_USER_AGENT\s*=\s*\n?\s*`\$\{DEFAULT_CODEX_ORIGINATOR\}\/\$\{DEFAULT_CODEX_USER_AGENT_VERSION\}`/,
  );
  assert.match(normalize, /gatewayUserAgent:\s*asString\([\s\S]*?gateway_user_agent/);
  assert.match(normalize, /gatewayUserAgentDefault:[\s\S]*?DEFAULT_CODEX_USER_AGENT/);
  assert.match(store, /gatewayUserAgent:\s*""/);
  assert.match(store, /gatewayUserAgentDefault:\s*DEFAULT_CODEX_USER_AGENT/);
  assert.match(settingsPage, /gatewayUserAgentInput=\{gatewayUserAgentInput\}/);
  assert.match(gatewayTab, /id="gateway-user-agent"/);
  assert.match(gatewayTab, /mutateAsync\(\{ gatewayUserAgent: nextUserAgent \}\)/);
  assert.match(gatewayTab, /当前聚合 API > 网关全局 > Codex 默认值|聚合 API 单独设置时优先/);
  assert.doesNotMatch(aggregatePage, /aggregateApiProbeUserAgent|probeSettingsOpen/);
});

test("Aggregate API User-Agent round-trips and preserves an empty override", async () => {
  const [types, normalize, accountClient, modal] = await Promise.all([
    readSource("src/types/api-key.ts"),
    readSource("src/lib/api/normalize.ts"),
    readSource("src/lib/api/account-client.ts"),
    readSource("src/components/modals/aggregate-api-modal.tsx"),
  ]);

  assert.match(types, /userAgent:\s*string \| null;/);
  assert.match(
    normalize,
    /userAgent:\s*asString\(source\.userAgent \?\? source\.user_agent\) \|\| null/,
  );
  assert.match(accountClient, /userAgent\?:\s*string \| null;/);
  assert.equal(
    accountClient.match(
      /typeof params\.userAgent === "string" \? params\.userAgent : null/g,
    )?.length,
    2,
  );
  assert.match(modal, /setUserAgent\(aggregateApi\?\.userAgent \|\| ""\)/);
  assert.equal(modal.match(/userAgent:\s*userAgent\.trim\(\)/g)?.length, 2);
  assert.match(modal, /id="aggregate-api-user-agent"/);
  assert.match(modal, /留空继承网关全局 User-Agent/);
});
