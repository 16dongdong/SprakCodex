import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const sourceRoot = path.resolve(import.meta.dirname, "..", "src");

/// 读取实际源码进行跨层契约检查；缺失文件直接使测试失败。
async function readSource(relativePath) {
  return fs.readFile(path.join(sourceRoot, relativePath), "utf8");
}

test("账号页同时提供本机接入和会话分流开关", async () => {
  const [pageSource, panelSource] = await Promise.all([
    readSource("app/accounts/page.tsx"),
    readSource("components/accounts/sessionRoutingPanel.tsx"),
  ]);
  assert.match(pageSource, /<SessionRoutingPanel accounts=\{accounts\}/);
  assert.match(panelSource, /observationClient\.start\(\)/);
  assert.match(panelSource, /observationClient\.stop\(\)/);
  assert.match(panelSource, /sessionRoutingClient\.setEnabled/);
  assert.match(panelSource, /sessionRoutingClient\.setAccountEnabled/);
});

test("桌面与 Web 命令映射保持会话分流 RPC 名称一致", async () => {
  const [clientSource, webCommandSource] = await Promise.all([
    readSource("lib/api/sessionRoutingClient.ts"),
    readSource("lib/api/transport-web-commands.ts"),
  ]);
  for (const method of [
    "sessionRouting/status",
    "sessionRouting/setEnabled",
    "sessionRouting/setAccountEnabled",
  ]) {
    assert.match(webCommandSource, new RegExp(method.replace("/", "\\/")));
  }
  assert.match(clientSource, /service_session_routing_status/);
  assert.match(clientSource, /service_session_routing_set_enabled/);
  assert.match(clientSource, /service_session_routing_set_account_enabled/);
});
