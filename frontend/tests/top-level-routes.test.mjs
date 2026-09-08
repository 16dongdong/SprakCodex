import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const sourcePath = path.resolve(
  import.meta.dirname,
  "..",
  "src",
  "lib",
  "app-shell",
  "top-level-routes.ts",
);

/// 将 TypeScript 路由配置编译成隔离模块，直接验证实际导出行为而不是复制配置。
async function loadTopLevelRoutesModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const testableSource = source
    .replace('"use client";', "")
    .replace(
      'import { normalizeRoutePath } from "@/lib/utils/static-routes";',
      "function normalizeRoutePath(value: string): string { return !value || value === '/' ? '/' : value.replace(/\\/+$/, ''); }",
    )
    .replace('import type { AppRole } from "@/types";', "type AppRole = string;");
  const compiled = ts.transpileModule(testableSource, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDirectory = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-personal-routes-"),
  );
  const modulePath = path.join(tempDirectory, "top-level-routes.mjs");
  await fs.writeFile(modulePath, compiled.outputText, "utf8");
  return import(pathToFileURL(modulePath).href);
}

const routes = await loadTopLevelRoutesModule();

test("个人版所有角色开放仪表盘、账号、请求日志和设置", () => {
  for (const role of ["system_admin", "admin", "member"]) {
    const access = { role, mode: "accounts", isDesktopRuntime: true };
    assert.deepEqual(
      routes.getAllowedTopLevelRouteSections(access).flatMap((section) =>
        section.routes.map((route) => route.path),
      ),
      ["/", "/accounts", "/logs", "/settings"],
    );
    assert.equal(routes.getFirstAllowedTopLevelRoutePath(access), "/");
  }
});

test("旧平台路由不再属于顶级功能", () => {
  const access = { role: "system_admin", mode: "accounts" };
  for (const pathValue of [
    "/account-manager",
    "/aggregate-api",
    "/apikeys",
    "/models",
    "/model-groups",
    "/platform-mode",
    "/plugins",
    "/projects",
    "/skills",
  ]) {
    assert.equal(routes.isTopLevelRouteAllowedForRole(pathValue, access), false);
  }
});

test("未知路径回到账号页，根路径恢复仪表盘", () => {
  assert.equal(routes.toTopLevelRoutePath("/removed"), "/accounts");
  assert.equal(routes.toTopLevelRoutePath("/"), "/");
  assert.equal(routes.getTopLevelRouteLabel("/accounts"), "账号");
});
