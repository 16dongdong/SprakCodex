#![allow(non_snake_case)]
use crate::commands::shared::rpc_call_in_background;

// 桌面端只转发结构化参数，节点协议解析和 mihomo 生命周期统一由服务进程负责。
#[tauri::command]
pub async fn service_proxy_runtime_config(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("proxyRuntime/config", addr, None).await
}

#[tauri::command]
pub async fn service_proxy_runtime_status(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("proxyRuntime/status", addr, None).await
}

#[tauri::command]
pub async fn service_proxy_runtime_save(
    addr: Option<String>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/save",
        addr,
        Some(serde_json::json!({ "config": config })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_parse(
    addr: Option<String>,
    text: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/parse",
        addr,
        Some(serde_json::json!({ "text": text })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_fetch(
    addr: Option<String>,
    url: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/fetch",
        addr,
        Some(serde_json::json!({ "url": url })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_select(
    addr: Option<String>,
    active: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/select",
        addr,
        Some(serde_json::json!({ "active": active })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_select_group(
    addr: Option<String>,
    group: String,
    member: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/selectGroup",
        addr,
        Some(serde_json::json!({ "group": group, "member": member })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_test(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("proxyRuntime/test", addr, None).await
}

#[tauri::command]
pub async fn service_proxy_runtime_test_node(
    addr: Option<String>,
    name: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "proxyRuntime/testNode",
        addr,
        Some(serde_json::json!({ "name": name })),
    )
    .await
}

#[tauri::command]
pub async fn service_proxy_runtime_egress(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("proxyRuntime/egress", addr, None).await
}
