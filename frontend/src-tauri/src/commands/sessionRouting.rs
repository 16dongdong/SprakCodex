#![allow(non_snake_case)]

use crate::commands::shared::rpc_call_in_background;

/// 查询会话分流总开关、账号参与状态和活跃绑定数量；后端错误原样交给界面展示。
#[tauri::command]
pub async fn service_session_routing_status(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sessionRouting/status", addr, None).await
}

/// 更新会话分流总开关；该操作只影响后续请求，不重写 Codex 配置。
#[tauri::command]
pub async fn service_session_routing_set_enabled(
    addr: Option<String>,
    enabled: bool,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "sessionRouting/setEnabled",
        addr,
        Some(serde_json::json!({ "enabled": enabled })),
    )
    .await
}

/// 更新单个账号是否参与新会话分配；已有绑定继续保留，避免会话上下文串号。
#[tauri::command]
pub async fn service_session_routing_set_account_enabled(
    addr: Option<String>,
    accountId: String,
    enabled: bool,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "sessionRouting/setAccountEnabled",
        addr,
        Some(serde_json::json!({ "accountId": accountId, "enabled": enabled })),
    )
    .await
}
