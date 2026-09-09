import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const repositoryRoot = resolve(import.meta.dirname, "../..");

// Windows 安装包只携带独立更新器；观测载荷由服务构建脚本链接进主程序，不再发布 DLL 资源。
test("观测载荷只存在于宿主可执行文件", () => {
  const configDirectory = resolve(repositoryRoot, "frontend/src-tauri");
  const config = JSON.parse(readFileSync(resolve(configDirectory, "tauri.windows.conf.json"), "utf8"));
  assert.deepEqual(config.bundle.resources, {
    "../updateAgent/target/release/updateAgent.exe": "updateAgent.exe",
  });
  assert.equal(config.bundle.windows.nsis.installerHooks, "windowsInstallerHooks.nsh");

  const runtimePaths = readFileSync(
    resolve(repositoryRoot, "backend/crates/service/src/directObservation/runtimePaths.rs"),
    "utf8",
  );
  const buildScript = readFileSync(resolve(repositoryRoot, "backend/crates/service/build.rs"), "utf8");
  const installerHooks = readFileSync(
    resolve(configDirectory, config.bundle.windows.nsis.installerHooks),
    "utf8",
  );
  assert.match(runtimePaths, /include_bytes!/);
  assert.match(buildScript, /codexmanager-direct-hook/);
  assert.doesNotMatch(JSON.stringify(config), /observationHook\d*\.dll|cphook\.dll/);
  assert.match(installerHooks, /Delete \/REBOOTOK/);
});

// 前端缓存复用只影响静态页面；Rust 构建脚本始终负责生成与链接载荷。
test("桌面预构建不再生成独立观测 DLL 资源", () => {
  const beforeBuild = readFileSync(
    resolve(repositoryRoot, "frontend/src-tauri/scripts/before-build.mjs"),
    "utf8",
  );
  assert.doesNotMatch(beforeBuild, /buildObservationHook/);
  assert.doesNotMatch(beforeBuild, /observationBuild/);
});
