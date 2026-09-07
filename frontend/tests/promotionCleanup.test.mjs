import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..", "..");

// 在源码回归中检查整条页面与代理链路；任一残留都会使测试失败，避免仅隐藏菜单后继续请求广告。
async function verifyRemovedPromotion() {
  const removedPaths = [
    "frontend/src/app/author",
    "frontend/src/lib/sponsor-links.ts",
    "frontend/public/sponsors",
    "docs/assets/images/sponsors",
    "backend/crates/service/src/app_settings/api/author_links.rs",
  ];
  for (const relativePath of removedPaths) {
    await assert.rejects(fs.access(path.join(repositoryRoot, relativePath)), { code: "ENOENT" });
  }
  const sourcePaths = [
    "frontend/src/lib/app-shell/top-level-routes.ts",
    "frontend/src/components/layout/page-keep-alive-viewport.tsx",
    "frontend/src/components/layout/sidebar.tsx",
    "frontend/src/lib/routes/root-page-paths.ts",
    "frontend/src/lib/runtime/runtime-capabilities.ts",
    "frontend/src/lib/api/normalize.ts",
    "frontend/src/lib/store/useAppStore.ts",
    "frontend/next.config.ts",
    "frontend/src-tauri/tauri.conf.json",
    "backend/crates/web/src/main.rs",
    "backend/crates/web/src/service_gateway.rs",
    "backend/crates/service/src/rpc_dispatch/app_settings.rs",
    "backend/crates/service/src/app_settings/api/current.rs",
    "backend/crates/service/src/app_settings/api/patch.rs",
  ];
  for (const relativePath of sourcePaths) {
    const source = await fs.readFile(path.join(repositoryRoot, relativePath), "utf8");
    assert.doesNotMatch(source, /author\.qxnm\.top|["']\/author["']|authorContent|author_content|authorSponsors|author_sponsors|authorServerRecommendations/, relativePath);
  }
}

// 独立仓库的更新必须留在当前维护者名下；检查默认地址和界面跳转，发现上游回流时失败。
async function verifyUpdateOwnership() {
  const sourcePaths = [
    "frontend/src-tauri/src/commands/updater/runtime.rs",
    "frontend/src/app/settings/settings-page-helpers.ts",
    "frontend/src/components/layout/automatic-update-checker.tsx",
  ];
  for (const relativePath of sourcePaths) {
    const source = await fs.readFile(path.join(repositoryRoot, relativePath), "utf8");
    assert.match(source, /16dongdong\/CodexManager/);
    assert.doesNotMatch(source, /qxcnm\/Codex-Manager/);
  }
}

test("推广页面、配置与代理链路均已移除", verifyRemovedPromotion);
test("更新入口仅使用独立仓库", verifyUpdateOwnership);
