use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

// 沿用统一管理员方法检查；状态、启停都在宿主内执行，不开放外部日志注入端点。
pub(super) fn dispatch(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "directObservation/status" => crate::directObservation::status(),
        "directObservation/start" => crate::directObservation::start(),
        "directObservation/stop" => crate::directObservation::stop(),
        _ => return None,
    };
    Some(super::response(req, super::value_or_error(result)))
}
