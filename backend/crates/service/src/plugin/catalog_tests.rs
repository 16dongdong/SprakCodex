use super::{builtin_catalog_entries, catalog_list_result, market_source_mode_for_request};
use codexmanager_core::rpc::types::{JsonRpcRequest, RequestId};

/// 函数 `catalog_request`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - params: 参数 params
///
/// # 返回
/// 返回函数执行结果
fn catalog_request(params: serde_json::Value) -> JsonRpcRequest {
    JsonRpcRequest {
        id: RequestId::from(1),
        method: "plugin/catalog/list".to_string(),
        params: Some(params),
        trace: None,
    }
}

/// 函数 `builtin_catalog_exposes_cleanup_plugins`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn builtin_catalog_exposes_cleanup_plugins() {
    let items = builtin_catalog_entries();
    assert_eq!(items.len(), 2);
    let banned = items
        .iter()
        .find(|item| item.id == "cleanup-banned-accounts")
        .expect("banned cleanup plugin");
    assert_eq!(banned.manifest_version, "1");
    assert_eq!(banned.category.as_deref(), Some("official"));
    assert_eq!(banned.runtime_kind, "rhai");
    assert!(banned
        .permissions
        .iter()
        .any(|item| item == "accounts:cleanup"));
    assert!(!banned.tags.is_empty());
    assert_eq!(banned.tasks.len(), 1);
    assert_eq!(banned.tasks[0].entrypoint, "run");
    assert_eq!(banned.tasks[0].schedule_kind, "interval");
    assert_eq!(
        banned.tasks[0].interval_seconds,
        Some(super::BUILTIN_CLEANUP_TASK_INTERVAL_SECS)
    );

    let unavailable_free = items
        .iter()
        .find(|item| item.id == "cleanup-unavailable-free-accounts")
        .expect("unavailable free cleanup plugin");
    assert_eq!(unavailable_free.manifest_version, "1");
    assert_eq!(unavailable_free.category.as_deref(), Some("official"));
    assert_eq!(unavailable_free.runtime_kind, "rhai");
    assert!(unavailable_free
        .permissions
        .iter()
        .any(|item| item == "accounts:cleanup"));
    assert!(!unavailable_free.tags.is_empty());
    assert_eq!(unavailable_free.tasks.len(), 1);
    assert_eq!(unavailable_free.tasks[0].entrypoint, "run");
    assert_eq!(unavailable_free.tasks[0].schedule_kind, "interval");
    assert_eq!(
        unavailable_free.tasks[0].interval_seconds,
        Some(super::BUILTIN_UNAVAILABLE_FREE_CLEANUP_TASK_INTERVAL_SECS)
    );
}

/// 函数 `request_market_mode_normalizes_private_to_custom`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn request_market_mode_normalizes_private_to_custom() {
    let request = catalog_request(serde_json::json!({
        "marketMode": "private"
    }));
    assert_eq!(market_source_mode_for_request(&request), "custom");
}

/// 函数 `custom_market_with_unreachable_source_returns_empty_items`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn custom_market_with_unreachable_source_returns_empty_items() {
    let request = catalog_request(serde_json::json!({
        "marketMode": "custom",
        "sourceUrl": "http://127.0.0.1:9/unreachable-plugin-market.json"
    }));
    let response = catalog_list_result(&request).expect("catalog response");
    let items = response
        .get("items")
        .and_then(serde_json::Value::as_array)
        .expect("items array");
    assert!(items.is_empty());
    assert_eq!(
        response
            .get("sourceUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        "http://127.0.0.1:9/unreachable-plugin-market.json"
    );
}

/// 函数 `custom_market_without_source_returns_empty_items`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn custom_market_without_source_returns_empty_items() {
    let request = catalog_request(serde_json::json!({
        "marketMode": "custom"
    }));
    let response = catalog_list_result(&request).expect("catalog response");
    let items = response
        .get("items")
        .and_then(serde_json::Value::as_array)
        .expect("items array");
    assert!(items.is_empty());
    assert_eq!(
        response
            .get("sourceUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        ""
    );
}

/// 函数 `builtin_market_never_uses_custom_source`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn builtin_market_never_uses_custom_source() {
    let request = catalog_request(serde_json::json!({
        "marketMode": "builtin",
        "sourceUrl": "http://127.0.0.1:48888/plugin-market.json"
    }));
    let response = catalog_list_result(&request).expect("catalog response");
    let items = response
        .get("items")
        .and_then(serde_json::Value::as_array)
        .expect("items array");
    assert_eq!(items.len(), 2);
    assert_eq!(
        response
            .get("sourceUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        ""
    );
}
