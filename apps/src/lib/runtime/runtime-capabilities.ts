import type { RuntimeCapabilities, RuntimeMode } from "@/types";

export const DEFAULT_WEB_RPC_BASE_URL = "/api/rpc";
export const DEFAULT_UNSUPPORTED_WEB_REASON =
  "当前页面缺少 CodexManager Web 运行壳，无法访问管理 RPC。请通过 codexmanager-web 打开，或在反向代理中转发 /api/rpc。";
export type RuntimeCapabilityView = {
  runtimeCapabilities: RuntimeCapabilities | null;
  mode: RuntimeMode;
  isDesktopRuntime: boolean;
  isUnsupportedWebRuntime: boolean;
  canAccessManagementRpc: boolean;
  canManageService: boolean;
  canSelfUpdate: boolean;
  canAutoStart: boolean;
  canCloseToTray: boolean;
  canOpenLocalDir: boolean;
  canUseBrowserFileImport: boolean;
  canUseBrowserDownloadExport: boolean;
};

/**
 * 函数 `asRecord`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

/**
 * 函数 `asString`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function asString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

/**
 * 函数 `asBoolean`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 * - fallback: 参数 fallback
 *
 * # 返回
 * 返回函数执行结果
 */
function asBoolean(value: unknown, fallback = false): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/**
 * 函数 `normalizeRpcBaseUrl`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
export function normalizeRpcBaseUrl(value: string | null | undefined): string {
  const normalized = String(value || "").trim();
  if (!normalized) {
    return "";
  }
  return normalized.endsWith("/")
    ? normalized.replace(/\/+$/, "") || DEFAULT_WEB_RPC_BASE_URL
    : normalized;
}

/**
 * 函数 `isRuntimeMode`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
export function isRuntimeMode(value: string): value is RuntimeMode {
  return (
    value === "desktop-tauri" ||
    value === "web-gateway" ||
    value === "unsupported-web"
  );
}

// 为桌面启动生成本地能力快照，不附带远程推广地址；返回值仅描述管理功能，无网络副作用。
export function buildDesktopRuntimeCapabilities(): RuntimeCapabilities {
  return {
    mode: "desktop-tauri",
    rpcBaseUrl: DEFAULT_WEB_RPC_BASE_URL,
    canManageService: true,
    canSelfUpdate: true,
    canAutoStart: true,
    canCloseToTray: true,
    canOpenLocalDir: true,
    canUseBrowserFileImport: true,
    canUseBrowserDownloadExport: true,
    unsupportedReason: null,
  };
}

// 根据 RPC 地址构建 Web 能力；空地址采用同源入口，不包含推广内容发现字段。
export function buildWebGatewayRuntimeCapabilities(
  rpcBaseUrl = DEFAULT_WEB_RPC_BASE_URL
): RuntimeCapabilities {
  return {
    mode: "web-gateway",
    rpcBaseUrl: normalizeRpcBaseUrl(rpcBaseUrl) || DEFAULT_WEB_RPC_BASE_URL,
    canManageService: false,
    canSelfUpdate: false,
    canAutoStart: false,
    canCloseToTray: false,
    canOpenLocalDir: false,
    canUseBrowserFileImport: true,
    canUseBrowserDownloadExport: true,
    unsupportedReason: null,
  };
}

// 为缺少运行壳的页面生成不可访问管理接口的状态，保留传入原因和 RPC 地址，不发起外部请求。
export function buildUnsupportedWebCapabilities(
  reason = DEFAULT_UNSUPPORTED_WEB_REASON,
  rpcBaseUrl = DEFAULT_WEB_RPC_BASE_URL
): RuntimeCapabilities {
  return {
    mode: "unsupported-web",
    rpcBaseUrl: normalizeRpcBaseUrl(rpcBaseUrl) || DEFAULT_WEB_RPC_BASE_URL,
    canManageService: false,
    canSelfUpdate: false,
    canAutoStart: false,
    canCloseToTray: false,
    canOpenLocalDir: false,
    canUseBrowserFileImport: false,
    canUseBrowserDownloadExport: false,
    unsupportedReason: reason,
  };
}

// 将运行壳响应收敛到管理能力白名单；旧响应中的推广字段不再透传，缺失字段沿用对应运行模式默认值。
export function normalizeRuntimeCapabilities(
  payload: unknown,
  fallbackRpcBaseUrl = DEFAULT_WEB_RPC_BASE_URL
): RuntimeCapabilities {
  const source = asRecord(payload) ?? {};
  const modeValue = asString(source.mode);
  const mode: RuntimeMode = isRuntimeMode(modeValue) ? modeValue : "web-gateway";
  const defaultCapabilities =
    mode === "desktop-tauri"
      ? buildDesktopRuntimeCapabilities()
      : mode === "unsupported-web"
        ? buildUnsupportedWebCapabilities(undefined, fallbackRpcBaseUrl)
        : buildWebGatewayRuntimeCapabilities(fallbackRpcBaseUrl);

  return {
    mode,
    rpcBaseUrl:
      normalizeRpcBaseUrl(asString(source.rpcBaseUrl)) ||
      defaultCapabilities.rpcBaseUrl,
    canManageService: asBoolean(
      source.canManageService,
      defaultCapabilities.canManageService
    ),
    canSelfUpdate: asBoolean(
      source.canSelfUpdate,
      defaultCapabilities.canSelfUpdate
    ),
    canAutoStart: asBoolean(
      source.canAutoStart,
      defaultCapabilities.canAutoStart
    ),
    canCloseToTray: asBoolean(
      source.canCloseToTray,
      defaultCapabilities.canCloseToTray
    ),
    canOpenLocalDir: asBoolean(
      source.canOpenLocalDir,
      defaultCapabilities.canOpenLocalDir
    ),
    canUseBrowserFileImport: asBoolean(
      source.canUseBrowserFileImport,
      defaultCapabilities.canUseBrowserFileImport
    ),
    canUseBrowserDownloadExport: asBoolean(
      source.canUseBrowserDownloadExport,
      defaultCapabilities.canUseBrowserDownloadExport
    ),
    unsupportedReason:
      asString(source.unsupportedReason) || defaultCapabilities.unsupportedReason || null,
  };
}

// 将运行时快照映射为组件能力视图；快照为空时按桌面标志选择默认状态，不再向组件暴露推广地址。
export function resolveRuntimeCapabilityView(
  runtimeCapabilities: RuntimeCapabilities | null,
  desktopFallback: boolean
): RuntimeCapabilityView {
  const resolvedCapabilities = runtimeCapabilities ??
    (desktopFallback
      ? buildDesktopRuntimeCapabilities()
      : buildUnsupportedWebCapabilities());
  const mode = resolvedCapabilities.mode;
  const isDesktopRuntime = mode === "desktop-tauri";

  return {
    runtimeCapabilities,
    mode,
    isDesktopRuntime,
    isUnsupportedWebRuntime: mode === "unsupported-web",
    canAccessManagementRpc: mode !== "unsupported-web",
    canManageService: resolvedCapabilities.canManageService,
    canSelfUpdate: resolvedCapabilities.canSelfUpdate,
    canAutoStart: resolvedCapabilities.canAutoStart,
    canCloseToTray: resolvedCapabilities.canCloseToTray,
    canOpenLocalDir: resolvedCapabilities.canOpenLocalDir,
    canUseBrowserFileImport: resolvedCapabilities.canUseBrowserFileImport,
    canUseBrowserDownloadExport: resolvedCapabilities.canUseBrowserDownloadExport,
  };
}
