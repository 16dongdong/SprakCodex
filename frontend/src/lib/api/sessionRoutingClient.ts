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

export const sessionRoutingClient = {
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
