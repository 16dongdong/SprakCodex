use crate::app_storage::apply_runtime_storage_env;
use crate::commands::shared::rpc_call_in_background;

/// 函数 `service_startup_snapshot`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - app: 参数 app
/// - addr: 参数 addr
/// - request_log_limit: 参数 request_log_limit
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_startup_snapshot(
    app: tauri::AppHandle,
    addr: Option<String>,
    request_log_limit: Option<i64>,
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
    include_api_models: Option<bool>,
    include_api_keys: Option<bool>,
    include_accounts: Option<bool>,
    include_usage_snapshots: Option<bool>,
    include_account_runtime: Option<bool>,
    include_account_details: Option<bool>,
) -> Result<serde_json::Value, String> {
    apply_runtime_storage_env(&app);
    let params = serde_json::json!({
        "requestLogLimit": request_log_limit,
        "dayStartTs": day_start_ts,
        "dayEndTs": day_end_ts,
        "includeApiModels": include_api_models,
        "includeApiKeys": include_api_keys,
        "includeAccounts": include_accounts,
        "includeUsageSnapshots": include_usage_snapshots,
        "includeAccountRuntime": include_account_runtime,
        "includeAccountDetails": include_account_details,
    });
    rpc_call_in_background("startup/snapshot", addr, Some(params)).await
}
