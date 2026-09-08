import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const repositoryRoot = resolve(import.meta.dirname, "../..");

// 从真实配置验证资源来源留在本仓库，安装目标是 DLL 基名，避免依赖外部预编译目录。
test("观测 DLL 打包路径位于本仓库且不携带父目录层次", () => {
  const configDirectory = resolve(repositoryRoot, "frontend/src-tauri");
  const config = JSON.parse(readFileSync(resolve(configDirectory, "tauri.windows.conf.json"), "utf8"));
  const [[source, target]] = Object.entries(config.bundle.resources);
  assert.equal(resolve(configDirectory, source), resolve(repositoryRoot, "backend/target/observationBuild/release/cphook.dll"));
  assert.equal(target, "observationHook9.dll");
  for (const crate of ["directHook", "directCommon"]) {
    assert.ok(existsSync(resolve(repositoryRoot, `backend/crates/${crate}/src/lib.rs`)));
  }
});

// 前端缓存复用不得绕过 DLL 编译；Windows 资源只进入对应平台配置。
test("桌面预构建在复用前端产物前编译本地 DLL", () => {
  const scripts = resolve(repositoryRoot, "frontend/src-tauri/scripts");
  const beforeBuild = readFileSync(resolve(scripts, "before-build.mjs"), "utf8");
  assert.ok(beforeBuild.indexOf('buildObservationHook(resolve(frontendDir, ".."))') < beforeBuild.indexOf('if (task === "build:desktop" && hasBuiltFrontendDist'));
  const buildHook = readFileSync(resolve(scripts, "buildObservationHook.mjs"), "utf8");
  assert.ok(buildHook.includes('"--locked"'));
  assert.ok(buildHook.includes('"codexmanager-direct-hook"'));
  assert.ok(buildHook.includes('resolve(backendRoot, "target/observationBuild")'));
  const commonConfig = JSON.parse(readFileSync(resolve(scripts, "../tauri.conf.json"), "utf8"));
  assert.equal(commonConfig.bundle.resources, undefined);
});
