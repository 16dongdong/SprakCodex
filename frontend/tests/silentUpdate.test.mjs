import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";

// 验证版本、仓库、独立资源和静默分支保持同一发布协议，阻止再次发布不可识别的标签。
test("更新配置统一标准版本与独立工作进程", async () => {
  const root = new URL("../../", import.meta.url);
  const config = JSON.parse(await fs.readFile(new URL("frontend/src-tauri/tauri.conf.json", root), "utf8"));
  const workerConfig = JSON.parse(await fs.readFile(new URL("frontend/src-tauri/tauri.windows.conf.json", root), "utf8"));
  const runtime = await fs.readFile(new URL("frontend/src-tauri/src/commands/updater/runtime.rs", root), "utf8");
  const worker = await fs.readFile(new URL("frontend/src-tauri/src/updateWatchdog.rs", root), "utf8");
  const ui = await fs.readFile(new URL("frontend/src/components/layout/automatic-update-checker.tsx", root), "utf8");
  assert.match(config.version, /^\d+\.\d+\.\d+$/);
  assert.ok(Object.values(workerConfig.bundle.resources).includes("updateAgent.exe"));
  assert.match(worker, /WatchdogCommand::Track/);
  assert.match(worker, /WatchdogCommand::Update/);
  assert.match(worker, /watchdog-ready/);
  assert.match(runtime, /16dongdong\/SprakCodex/);
  assert.match(ui, /appSettings.silentUpdate/);
  assert.match(ui, /applySilentUpdate/);
  assert.ok(ui.indexOf("appSettings.silentUpdate") < ui.indexOf("appClient.showMainWindow()"));
});
