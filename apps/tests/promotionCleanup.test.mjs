import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..", "..");

// 在源码回归中检查整条页面与代理链路；任一残留都会使测试失败，避免仅隐藏菜单后继续请求广告。
async function verifyRemovedPromotion() {
  const removedPaths = [
    "apps/src/app/author",
    "apps/src/lib/sponsor-links.ts",
    "apps/public/sponsors",
    "assets/images/sponsors",
    "crates/service/src/app_settings/api/author_links.rs",
  ];
  for (const relativePath of removedPaths) {
    await assert.rejects(fs.access(path.join(repositoryRoot, relativePath)), { code: "ENOENT" });
  }
  const sourcePaths = [
    "apps/src/lib/app-shell/top-level-routes.ts",
    "apps/src/components/layout/page-keep-alive-viewport.tsx",
    "apps/src/components/layout/sidebar.tsx",
    "apps/src/lib/routes/root-page-paths.ts",
    "apps/src/lib/runtime/runtime-capabilities.ts",
    "apps/src/lib/api/normalize.ts",
    "apps/src/lib/store/useAppStore.ts",
    "apps/next.config.ts",
    "apps/src-tauri/tauri.conf.json",
    "crates/web/src/main.rs",
    "crates/web/src/service_gateway.rs",
    "crates/service/src/rpc_dispatch/app_settings.rs",
    "crates/service/src/app_settings/api/current.rs",
    "crates/service/src/app_settings/api/patch.rs",
  ];
  for (const relativePath of sourcePaths) {
    const source = await fs.readFile(path.join(repositoryRoot, relativePath), "utf8");
    assert.doesNotMatch(source, /author\.qxnm\.top|["']\/author["']|authorContent|author_content|authorSponsors|author_sponsors|authorServerRecommendations/, relativePath);
  }
}

// 独立仓库的更新必须留在当前维护者名下；检查默认地址和界面跳转，发现上游回流时失败。
async function verifyUpdateOwnership() {
  const sourcePaths = [
    "apps/src-tauri/src/commands/updater/runtime.rs",
    "apps/src/app/settings/settings-page-helpers.ts",
    "apps/src/components/layout/automatic-update-checker.tsx",
  ];
  for (const relativePath of sourcePaths) {
    const source = await fs.readFile(path.join(repositoryRoot, relativePath), "utf8");
    assert.match(source, /16dongdong\/CodexManager/);
    assert.doesNotMatch(source, /qxcnm\/Codex-Manager/);
  }
}

test("推广页面、配置与代理链路均已移除", verifyRemovedPromotion);
test("更新入口仅使用独立仓库", verifyUpdateOwnership);
