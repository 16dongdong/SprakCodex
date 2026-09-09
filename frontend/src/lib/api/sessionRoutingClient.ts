import { invoke, withAddr } from "./transport";

export interface AccountRoutingPreference {
  accountId: string;
  enabled: boolean;
  activeBindingCount: number;
}

export interface SessionRoutingStatus {
  enabled: boolean;
  activeBindingCount: number;
  accounts: AccountRoutingPreference[];
}

export const sessionRoutingQueryKey = ["sessionRouting", "status"] as const;

export interface RoutingSession {
  title?: string | null; projectPath?: string | null;
  sessionId: string; accountId: string | null; accountLabel: string | null;
  status: string; createdAt: number; lastUsedAt: number; reason: string;
  requestedAccountId: string | null; requestedAccountLabel: string | null;
}
export interface RoutingSessionPage {
  projects: string[]; metadataWarning?: string | null;
  items: RoutingSession[]; total: number; page: number; pageSize: number;
  accounts: Array<{id: string; label: string}>;
}
export const sessionRoutingClient = {
  // 会话列表按服务和筛选条件分页，不从请求日志猜测绑定。
  list: (page: number, search: string, project = "") => invoke<RoutingSessionPage>("service_session_routing_list", withAddr({page,search,project})),
  // 重置只影响路由记录，原 Codex 会话内容保留。
  reset: (sessionId: string) => invoke<{ok:boolean}>("service_session_routing_reset",withAddr({sessionId})),
  // 目标账号由服务端复核，客户端不乐观修改当前绑定。
  switchAccount: (sessionId: string,accountId: string) => invoke<{ok:boolean}>("service_session_routing_switch",withAddr({sessionId,accountId})),
  /// 查询服务端持久化状态，不使用浏览器缓存推断当前开关。
  status: () =>
    invoke<SessionRoutingStatus>("service_session_routing_status", withAddr()),

  /// 总开关只影响后续请求；正在传输的请求继续使用已确定的身份快照。
  setEnabled: (enabled: boolean) =>
    invoke<SessionRoutingStatus>(
      "service_session_routing_set_enabled",
      withAddr({ enabled }),
    ),

  /// 账号参与开关只控制新绑定，已有绑定由后端保持并在不可用时明确报错。
  setAccountEnabled: (accountId: string, enabled: boolean) =>
    invoke<SessionRoutingStatus>(
      "service_session_routing_set_account_enabled",
      withAddr({ accountId, enabled }),
    ),
};
