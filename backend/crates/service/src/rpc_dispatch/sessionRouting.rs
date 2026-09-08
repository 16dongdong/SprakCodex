use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

/// 分发个人版会话分流命令；所有写操作均返回最新完整状态，避免客户端维护第二份真相。
pub(super) fn dispatch(request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match request.method.as_str() {
        "sessionRouting/status" => crate::sessionRouting::status(),
        "sessionRouting/setEnabled" => crate::sessionRouting::setEnabled(
            super::bool_param(request, "enabled").unwrap_or(false),
        ),
        "sessionRouting/setAccountEnabled" => {
            let accountId = super::str_param(request, "accountId").unwrap_or_default();
            let enabled = super::bool_param(request, "enabled").unwrap_or(false);
            crate::sessionRouting::setAccountEnabled(accountId, enabled)
        }
        _ => return None,
    };
    Some(super::response(request, super::value_or_error(result)))
}
