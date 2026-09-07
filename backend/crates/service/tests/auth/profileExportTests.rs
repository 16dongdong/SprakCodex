#![allow(non_snake_case)]

use super::*;

// 导出时间不能伪装成刷新时间，否则接收方会错误推迟维护；仅检查内存 JSON，不接触本机登录文件。
#[test]
fn exportedAuthPreservesActualRefreshTime() {
    let account = AccountDirectAuthProfile {
        id: "fixture-account".to_string(),
        issuer: "http://127.0.0.1".to_string(),
        chatgpt_account_id: Some("fixture-workspace".to_string()),
        status: "active".to_string(),
    };
    let token = Token {
        account_id: account.id.clone(),
        id_token: "ID_TOKEN_FIXTURE".to_string(),
        access_token: "ACCESS_TOKEN_FIXTURE".to_string(),
        refresh_token: "REFRESH_TOKEN_FIXTURE".to_string(),
        api_key_access_token: None,
        last_refresh: 123,
    };
    let exported: serde_json::Value =
        serde_json::from_str(&build_direct_auth_json(&account, &token).unwrap()).unwrap();
    let timestamp =
        chrono::DateTime::parse_from_rfc3339(exported["last_refresh"].as_str().unwrap()).unwrap();
    assert_eq!(timestamp.timestamp(), token.last_refresh);
    assert_eq!(exported["tokens"]["account_id"], "fixture-workspace");
}
