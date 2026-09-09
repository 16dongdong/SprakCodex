use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

/// 分发个人版会话分流命令；所有写操作均返回最新完整状态，避免客户端维护第二份真相。
pub(super) fn dispatch(request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let management = match request.method.as_str() {
        "sessionRouting/list" => Some(crate::sessionRouting::listSessions(
            request
                .params
                .as_ref()
                .and_then(|p| p.get("page"))
                .and_then(|v| v.as_i64())
                .unwrap_or(1),
            super::str_param(request, "search").unwrap_or_default(),
            super::str_param(request, "project").unwrap_or_default(),
        )),
        "sessionRouting/reset" => Some(crate::sessionRouting::resetSession(
            &super::str_param(request, "sessionId").unwrap_or_default(),
        )),
        "sessionRouting/switch" => Some(crate::sessionRouting::switchSession(
            &super::str_param(request, "sessionId").unwrap_or_default(),
            &super::str_param(request, "accountId").unwrap_or_default(),
        )),
        _ => None,
    };
    if let Some(result) = management {
        return Some(super::response(request, super::value_or_error(result)));
    }
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
