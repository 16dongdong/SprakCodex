"use client";

import type { AppRole, AppSessionResult } from "@/types";

export const APP_SESSION_QUERY_KEY = ["personal-session"] as const;

const PERSONAL_SESSION = {
  mode: "personal",
  currentUser: null,
  role: "system_admin",
  permissions: ["system:admin", "requestlog:self", "profile:self"],
  distributionEnabled: false,
  billingModeLock: { accountModeLocked: true, distributionLocked: true, reasons: ["personal_mode"] },
} satisfies AppSessionResult;

export function isAdminRole(role: AppRole | string | null | undefined): boolean {
  return role === "admin" || role === "system_admin";
}

/// 涓汉鐗堝缁堣В鏋愪负鏈満绠＄悊鍛橈紝淇濈暀鍙傛暟浠呭吋瀹圭幇鏈夎皟鐢ㄧ鍚嶃€?
export function resolveSessionRole(
  _session: AppSessionResult | null | undefined,
  _isLoading = false,
  _forceSystemAdmin = false,
): AppRole {
  void _session;
  void _isLoading;
  void _forceSystemAdmin;
  return "system_admin";
}

/// 杩斿洖涓嶅彲鍙樼殑鏈満韬唤蹇収锛屼笉鍐嶅悜鍚庣璇锋眰鐢ㄦ埛銆佽鑹层€侀挶鍖呮垨鎴愬憳浼氳瘽銆?
export function useAppSession(_options: { enabled?: boolean } = {}) {
  void _options;
  return {
    data: PERSONAL_SESSION,
    isLoading: false,
    isError: false,
    error: null,
    isServiceReady: true,
    isSessionQueryEnabled: false,
  };
}
