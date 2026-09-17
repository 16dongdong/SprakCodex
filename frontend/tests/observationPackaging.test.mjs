import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const repositoryRoot = resolve(import.meta.dirname, "../..");

// Windows 安装包携带后台服务与独立更新器；观测 DLL 只链接进服务，不作为独立资源发布。
test("观测载荷只存在于宿主可执行文件", () => {
  const configDirectory = resolve(repositoryRoot, "frontend/src-tauri");
  const config = JSON.parse(readFileSync(resolve(configDirectory, "tauri.windows.conf.json"), "utf8"));
  assert.deepEqual(config.bundle.resources, {
    "../../backend/target/release/SprakCodex-service.exe": "SprakCodex-service.exe",
    "../../third_party/mihomo/mihomo.exe": "mihomo.exe",
    "../../third_party/mihomo/LICENSE": "licenses/mihomo-GPL-3.0.txt",
    "../../third_party/mihomo/README.md": "licenses/mihomo-README.md",
    "../updateAgent/target/release/updateAgent.exe": "updateAgent.exe",
  });
  const runtimePaths = readFileSync(
    resolve(repositoryRoot, "backend/crates/service/src/directObservation/runtimePaths.rs"),
    "utf8",
  );
  const buildScript = readFileSync(resolve(repositoryRoot, "backend/crates/service/build.rs"), "utf8");
  assert.match(runtimePaths, /include_bytes!/);
  assert.match(buildScript, /codexmanager-direct-hook/);
  assert.doesNotMatch(JSON.stringify(config), /observationHook\d*\.dll|cphook\.dll/);
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
