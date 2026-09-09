import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "../..");

// 从测试文件定位仓库边界，核对各工程入口；缺失或迁移回旧目录时直接失败。
async function verifyProjectEntrypoints() {
  for (const relativePath of [
    "frontend/package.json",
    "frontend/src-tauri/Cargo.toml",
    "backend/Cargo.toml",
    "backend/Cargo.lock",
    "backend/.cargo/config.toml",
    "backend/scripts/ci/check-websocket-pins.sh",
    "backend/docker/Dockerfile.web",
    "docs/projectLayout.md",
  ]) {
    await fs.access(path.join(repositoryRoot, relativePath));
  }
  for (const relativePath of ["apps/package.json", "Cargo.toml", "crates/service/Cargo.toml"]) {
    await assert.rejects(fs.access(path.join(repositoryRoot, relativePath)), { code: "ENOENT" });
  }
}

// 验证桌面壳仅通过显式路径依赖服务端，Web 构建只读取约定的静态导出目录。
async function verifyBuildBoundaries() {
  const desktopRoot = path.join(repositoryRoot, "frontend/src-tauri");
  const desktopManifest = await fs.readFile(path.join(desktopRoot, "Cargo.toml"), "utf8");
  for (const dependency of desktopManifest.matchAll(/path\s*=\s*"([^"]+)"/g)) {
    if (dependency[1].endsWith(".rs")) continue;
    const manifestPath = path.resolve(desktopRoot, dependency[1], "Cargo.toml");
    assert.ok(manifestPath.startsWith(path.join(repositoryRoot, "backend") + path.sep) || manifestPath === path.join(repositoryRoot, "frontend", "updateAgent", "Cargo.toml"));
    await fs.access(manifestPath);
  }
  const webRoot = path.join(repositoryRoot, "backend/crates/web");
  const buildSource = await fs.readFile(path.join(webRoot, "build.rs"), "utf8");
  const distMatch = buildSource.match(/let dist_dir = manifest_dir.join\("([^"]+)"\)/);
  assert.ok(distMatch, "Web 构建应声明静态导出目录");
  assert.equal(path.resolve(webRoot, distMatch[1]), path.join(repositoryRoot, "frontend/out"));
}

test("前后端工程入口位于各自目录", verifyProjectEntrypoints);
test("桌面依赖和静态导出边界使用新路径", verifyBuildBoundaries);
