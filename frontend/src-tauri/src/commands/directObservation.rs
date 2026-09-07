#![allow(non_snake_case)]
use crate::commands::shared::rpc_call_in_background;

// 桌面状态查询走既有认证 RPC；addr 指向当前后端，后台执行失败由前端统一显示。
#[tauri::command]
pub async fn service_observation_status(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("directObservation/status", addr, None).await
}

// 只在用户点击后启用宿主内的观测运行时，不启动任何外部捕获程序。
#[tauri::command]
pub async fn service_observation_start(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("directObservation/start", addr, None).await
}

// 等待监听关闭与写库完成，再将最终结果返回 UI；重复停止由后端幂等处理。
#[tauri::command]
pub async fn service_observation_stop(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("directObservation/stop", addr, None).await
}
