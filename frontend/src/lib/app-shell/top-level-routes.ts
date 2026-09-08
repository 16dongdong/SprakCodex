"use client";

import { normalizeRoutePath } from "@/lib/utils/static-routes";
import type { AppRole } from "@/types";

type TopLevelRouteSectionId = "personal";

const ROUTE_SECTION_LABELS: Record<TopLevelRouteSectionId, string> = {
  personal: "个人工具",
};

// 根路径恢复个人仪表盘，与账号、日志和设置共享页面缓存及本机访问范围。
export const TOP_LEVEL_ROUTE_CONFIG = [
  {
    path: "/",
    label: "仪表盘",
    section: "personal",
    roles: ["system_admin", "admin", "member"],
  },
  {
    path: "/accounts",
    label: "账号",
    section: "personal",
    roles: ["system_admin", "admin", "member"],
  },
  {
    path: "/logs",
    label: "请求日志",
    section: "personal",
    roles: ["system_admin", "admin", "member"],
  },
  {
    path: "/settings",
    label: "设置",
    section: "personal",
    roles: ["system_admin", "admin", "member"],
  },
] as const;

export type TopLevelRoutePath = (typeof TOP_LEVEL_ROUTE_CONFIG)[number]["path"];
export type TopLevelRouteConfig = (typeof TOP_LEVEL_ROUTE_CONFIG)[number];

export interface TopLevelRouteAccessContext {
  role?: AppRole | string | null;
  mode?: string | null;
  isDesktopRuntime?: boolean | null;
}

export type TopLevelRouteAccess =
  | AppRole
  | string
  | null
  | undefined
  | TopLevelRouteAccessContext;

interface NormalizedTopLevelRouteAccessContext {
  role: string;
}

export interface TopLevelRouteSection {
  id: TopLevelRouteSectionId;
  label: string;
  routes: TopLevelRouteConfig[];
}

const TOP_LEVEL_ROUTE_SET = new Set<TopLevelRoutePath>(
  TOP_LEVEL_ROUTE_CONFIG.map((route) => route.path),
);

function normalizeRole(role: AppRole | string | null | undefined): string {
  return role || "system_admin";
}

function isTopLevelRouteAccessContext(
  access: TopLevelRouteAccess,
): access is TopLevelRouteAccessContext {
  return Boolean(access && typeof access === "object" && !Array.isArray(access));
}

/// 个人版四个页面对本机用户一致开放；保留角色读取只为兼容现有登录快照。
function normalizeAccessContext(
  access: TopLevelRouteAccess,
): NormalizedTopLevelRouteAccessContext {
  return {
    role: normalizeRole(isTopLevelRouteAccessContext(access) ? access.role : access),
  };
}

function isRouteAllowedForAccess(
  route: TopLevelRouteConfig,
  access: NormalizedTopLevelRouteAccessContext,
): boolean {
  return (route.roles as readonly string[]).includes(access.role);
}

export function isAdminTopLevelRole(
  role: AppRole | string | null | undefined,
): boolean {
  const normalizedRole = normalizeRole(role);
  return normalizedRole === "system_admin" || normalizedRole === "admin";
}

export function isTopLevelRoutePath(path: string): path is TopLevelRoutePath {
  return TOP_LEVEL_ROUTE_SET.has(normalizeRoutePath(path) as TopLevelRoutePath);
}

export function toTopLevelRoutePath(path: string): TopLevelRoutePath {
  const normalizedPath = normalizeRoutePath(path);
  return isTopLevelRoutePath(normalizedPath) ? normalizedPath : "/accounts";
}

export function getTopLevelRouteLabel(
  path: string,
  _access?: TopLevelRouteAccess,
): string {
  void _access;
  const normalizedPath = normalizeRoutePath(path);
  return (
    TOP_LEVEL_ROUTE_CONFIG.find((item) => item.path === normalizedPath)?.label ??
    "CodexManager"
  );
}

export function isTopLevelRouteAllowedForRole(
  path: string,
  access: TopLevelRouteAccess,
  _mode?: string | null,
): boolean {
  void _mode;
  const normalizedPath = normalizeRoutePath(path);
  const route = TOP_LEVEL_ROUTE_CONFIG.find((item) => item.path === normalizedPath);
  return Boolean(route && isRouteAllowedForAccess(route, normalizeAccessContext(access)));
}

export function getAllowedTopLevelRoutes(
  access: TopLevelRouteAccess,
  _mode?: string | null,
) {
  void _mode;
  const normalizedAccess = normalizeAccessContext(access);
  return TOP_LEVEL_ROUTE_CONFIG.filter((route) =>
    isRouteAllowedForAccess(route, normalizedAccess),
  );
}

export function getAllowedTopLevelRouteSections(
  access: TopLevelRouteAccess,
  _mode?: string | null,
): TopLevelRouteSection[] {
  void _mode;
  const routes = getAllowedTopLevelRoutes(access).filter(
    (route) => !("navigationHidden" in route && route.navigationHidden),
  );
  return routes.length === 0
    ? []
    : [{ id: "personal", label: ROUTE_SECTION_LABELS.personal, routes }];
}

export function getFirstAllowedTopLevelRoutePath(
  access: TopLevelRouteAccess,
  _mode?: string | null,
): TopLevelRoutePath {
  void _mode;
  return (
    getAllowedTopLevelRoutes(access).find(
      (route) => !("navigationHidden" in route && route.navigationHidden),
    )?.path ?? "/accounts"
  );
}
