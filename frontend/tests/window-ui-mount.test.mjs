import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

async function readSource(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

test("读取设置不会关闭正在启动的托盘预览", async () => {
  const source = await readSource("src-tauri/src/commands/settings/ui.rs");
  const getStart = source.indexOf("pub async fn app_settings_get");
  const setStart = source.indexOf("pub async fn app_settings_set");

  assert.ok(getStart >= 0 && setStart > getStart, "settings command bodies missing");
  assert.doesNotMatch(
    source.slice(getStart, setStart),
    /sync_window_ui_mount_state\(&app\)/,
  );
  assert.match(source.slice(setStart), /sync_window_ui_mount_state\(&app\)/);
});

test("后台保活由新的窗口常驻设置决定", async () => {
  const source = await readSource(
    "src-tauri/src/commands/settings/tray_state.rs",
  );

  assert.match(
    source,
    /effective_lightweight_mode_on_close_to_tray\(\s*!keep_window_ui_mounted,\s*effective_close_to_tray,\s*\)/,
  );
  assert.match(
    source,
    /"lightweightModeOnCloseToTray"\.to_string\(\),\s*serde_json::json!\(!keep_window_ui_mounted\)/,
  );
});

test("桌面恢复优先复用服务，托盘预览只连接服务", async () => {
  const source = await readSource("src/components/layout/app-bootstrap.tsx");

  assert.match(
    source,
    /const initializeOrStartService = useCallback\([\s\S]*?return await initializeService\(addr, 0\);[\s\S]*?return startAndInitializeService\(addr\)/,
  );
  assert.match(
    source,
    /const connectToDesktopService = isTrayPreview\s*\? initializeService\(addr, TRAY_PREVIEW_SERVICE_INITIALIZE_RETRIES\)\s*: initializeOrStartService\(addr\)/,
  );
  assert.match(source, /TRAY_PREVIEW_SERVICE_INITIALIZE_RETRIES = 40/);
});

test("窗口常驻设置明确依赖关闭到托盘并展示资源取舍", async () => {
  const source = await readSource(
    "src/app/settings/components/general-basics-card.tsx",
  );

  assert.match(source, /!snapshot\.closeToTrayOnClose/);
  assert.match(source, /快速唤醒：关闭后隐藏并保留界面/);
  assert.match(source, /低资源：关闭后销毁界面，后台服务继续运行/);
  assert.match(source, /checked=\{snapshot\.keepWindowUiMounted\}/);
  assert.match(
    source,
    /checked=\{snapshot\.keepWindowUiMounted\}[\s\S]*?disabled=\{[\s\S]*?!canCloseToTray[\s\S]*?!snapshot\.closeToTraySupported[\s\S]*?!snapshot\.closeToTrayOnClose[\s\S]*?\}/,
  );
  assert.match(
    source,
    /updateSettings\.mutate\(\{ keepWindowUiMounted: value \}\)/,
  );
});

test("启动主窗口设置在基础设置中持久化", async () => {
  const source = await readSource(
    "src/app/settings/components/general-basics-card.tsx",
  );

  assert.match(source, /启动时显示主界面/);
  assert.match(source, /checked=\{snapshot\.showMainWindowOnStartup\}/);
  assert.match(
    source,
    /updateSettings\.mutate\(\{ showMainWindowOnStartup: value \}\)/,
  );
});
