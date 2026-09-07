"use client";

import { normalizeRoutePath } from "@/lib/utils/static-routes";
import { useAppStore } from "@/lib/store/useAppStore";

/**
 * 函数 `useDesktopPageActive`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - expectedPath: 参数 expectedPath
 *
 * # 返回
 * 返回函数执行结果
 */
export function useDesktopPageActive(expectedPath: string): boolean {
  const currentShellPath = useAppStore((state) => state.currentShellPath);
  return currentShellPath === normalizeRoutePath(expectedPath);
}
