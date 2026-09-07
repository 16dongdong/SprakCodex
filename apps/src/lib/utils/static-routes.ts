"use client";

/**
 * 函数 `normalizeRoutePath`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - path: 参数 path
 *
 * # 返回
 * 返回函数执行结果
 */
export function normalizeRoutePath(path: string): string {
  if (!path || path === "/") {
    return "/";
  }
  return path.replace(/\/+$/, "");
}

/**
 * 函数 `looksLikeAssetPath`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - pathname: 参数 pathname
 *
 * # 返回
 * 返回函数执行结果
 */
function looksLikeAssetPath(pathname: string): boolean {
  const lastSegment = pathname.split("/").pop() || "";
  return lastSegment.includes(".");
}

/**
 * 函数 `buildStaticRouteUrl`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - pathname: 参数 pathname
 * - search: 参数 search
 * - hash: 参数 hash
 *
 * # 返回
 * 返回函数执行结果
 */
export function buildStaticRouteUrl(
  pathname: string,
  search = "",
  hash = "",
): string {
  if (!pathname || pathname === "/") {
    return `/${search}${hash}`;
  }

  if (pathname.endsWith("/") || looksLikeAssetPath(pathname)) {
    return `${pathname}${search}${hash}`;
  }

  return `${pathname}/${search}${hash}`;
}

/**
 * 函数 `getCanonicalStaticRouteUrl`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * 无
 *
 * # 返回
 * 返回函数执行结果
 */
export function getCanonicalStaticRouteUrl(): string | null {
  if (typeof window === "undefined") {
    return null;
  }

  const { pathname, search, hash } = window.location;
  const canonical = buildStaticRouteUrl(pathname, search, hash);
  const current = `${pathname}${search}${hash}`;
  return canonical === current ? null : canonical;
}
