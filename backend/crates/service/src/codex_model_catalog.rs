use codexmanager_core::rpc::types::ModelsResponse;
use codexmanager_core::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayCatalogPolicy {
    Managed,
}

/// 返回费用估算仍需使用的内部模型元数据；不写入 Codex 配置或模型缓存。
pub(crate) fn models_response_for_gateway_key(
    storage: &Storage,
    _api_key_id: &str,
) -> Result<(ModelsResponse, GatewayCatalogPolicy), String> {
    Ok((
        crate::models_v2::models_response_with_storage(storage)?,
        GatewayCatalogPolicy::Managed,
    ))
}
