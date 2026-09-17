use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

// 代理 RPC 只传结构化节点配置；内核密钥和动态 mixed-port 的控制密钥永不离开服务进程。
pub(super) fn dispatch(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "proxyRuntime/config" => super::value_or_error(crate::embeddedProxy::loadConfig()),
        "proxyRuntime/status" => super::value_or_error(crate::embeddedProxy::status()),
        "proxyRuntime/save" => {
            let config = req
                .params
                .as_ref()
                .and_then(|params| params.get("config"))
                .cloned()
                .ok_or_else(|| "缺少代理配置".to_string())
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|error| format!("代理配置无效：{error}"))
                })
                .and_then(crate::embeddedProxy::saveConfig);
            super::value_or_error(config)
        }
        "proxyRuntime/parse" => super::as_json(crate::embeddedProxy::parseClashText(
            super::str_param(req, "text").unwrap_or_default(),
        )),
        "proxyRuntime/fetch" => super::value_or_error(crate::embeddedProxy::fetchSubscription(
            super::str_param(req, "url").unwrap_or_default(),
        )),
        "proxyRuntime/select" => super::value_or_error(
            crate::embeddedProxy::select(super::str_param(req, "active").unwrap_or_default())
                .and_then(|_| crate::embeddedProxy::status()),
        ),
        "proxyRuntime/selectGroup" => super::value_or_error(
            crate::embeddedProxy::selectGroupMember(
                super::str_param(req, "group").unwrap_or_default(),
                super::str_param(req, "member").unwrap_or_default(),
            )
            .and_then(|_| crate::embeddedProxy::status()),
        ),
        "proxyRuntime/test" => super::value_or_error(crate::embeddedProxy::testNodes()),
        "proxyRuntime/egress" => super::value_or_error(crate::embeddedProxy::testEgress()),
        _ => return None,
    };
    Some(super::response(req, result))
}
