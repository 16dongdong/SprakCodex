import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const root = path.resolve(import.meta.dirname, "../..");
const page = await fs.readFile(path.join(root, "frontend/src/app/proxy/page.tsx"), "utf8");
const adapter = await fs.readFile(path.join(root, "frontend/src/app/proxy/invokeProxy.ts"), "utf8");
const kernel = await fs.readFile(path.join(root, "backend/crates/service/src/embeddedProxy/kernel.rs"), "utf8");
const config = await fs.readFile(path.join(root, "backend/crates/service/src/embeddedProxy/config.rs"), "utf8");
const bundle = JSON.parse(await fs.readFile(path.join(root, "frontend/src-tauri/tauri.windows.conf.json"), "utf8"));

test("代理页覆盖节点、订阅、代理组和链式入口", () => {
  for (const marker of ["fetch_subscription", "ProxyGroup", "chain_entry", '"url-test"', "test_nodes"]) {
    assert.match(page, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
  assert.match(adapter, /proxyRuntimeClient\.fetch/);
  assert.match(page, /data-proxy-group/);
  assert.match(page, /onPointerMove/);
  assert.match(page, /"test_node"/);
});

test("内置内核保留全协议字段并生成 mixed-port 配置", () => {
  assert.match(config, /serde\(flatten\)/);
  assert.match(kernel, /"mixed-port"/);
  assert.match(kernel, /"dialer-proxy"/);
  assert.match(kernel, /parse_clash_subscription/);
  assert.match(kernel, /chatgpt\.com%2Fcdn-cgi%2Ftrace/);
  assert.match(kernel, /\/proxies\/\{\}\/delay/);
  assert.equal(bundle.bundle.resources["../../third_party/mihomo/mihomo.exe"], "mihomo.exe");
});
