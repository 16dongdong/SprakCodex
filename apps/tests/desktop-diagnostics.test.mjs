import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

async function read(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

test("desktop diagnostics keep startup failures visible and file logs bounded", async () => {
  const [runtime, diagnostics, updater, registry, api, card] = await Promise.all([
    read("src-tauri/src/lib.rs"),
    read("src-tauri/src/desktop_diagnostics.rs"),
    read("src-tauri/src/commands/updater/mod.rs"),
    read("src-tauri/src/commands/registry.rs"),
    read("src/lib/api/desktop-diagnostics.ts"),
    read("src/app/settings/components/desktop-diagnostics-card.tsx"),
  ]);

  assert.match(runtime, /RotationStrategy::KeepOne/);
  assert.match(runtime, /max_file_size\(512 \* 1024\)/);
  assert.match(runtime, /create_pre_migration_backup/);
  assert.match(diagnostics, /STARTUP_ERROR_FILE_NAME: &str = "startup-error\.log"/);
  assert.match(diagnostics, /command_line_debug_requested/);
  assert.match(updater, /file_logging_enabled\(\)/);
  assert.match(updater, /UPDATE_LOG_MAX_FILE_SIZE/);

  for (const command of [
    "app_diagnostics_settings_get",
    "app_diagnostics_settings_set",
    "app_diagnostics_open_logs_dir",
  ]) {
    assert.match(registry, new RegExp(`::${command}\\b`));
    assert.match(api, new RegExp(`"${command}"`));
  }

  assert.match(card, /请求日志、Token 与费用统计不受影响/);
  assert.match(card, /最近一次启动失败/);
});
