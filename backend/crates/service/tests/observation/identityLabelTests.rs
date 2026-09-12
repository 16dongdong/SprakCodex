use super::*;
use base64::Engine;
use codexmanager_core::storage::{Account, Token};

// 展示身份来自同一请求的令牌声明，但原始令牌不进入记录；名称缺失时不伪装成密钥名称。
#[test]
fn extractsEmailAndNameWithoutKeepingCredential() {
    let claims = serde_json::json!({"sub":"fixture-user","https://api.openai.com/profile":{"name":"Fixture Name","email":"fixture@example.com"}});
    let token = format!(
        "e30.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let mut headers = hyper::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    headers.insert("chatgpt-account-id", "fixture-account".parse().unwrap());
    let mut exchange = Exchange::new("chatgpt.com", "/backend-api/codex/responses", "websocket");
    exchange.captureHeaders(&headers);
    assert_eq!(
        exchange.accountLabel.as_deref(),
        Some("Fixture Name <fixture@example.com>")
    );
    assert_eq!(exchange.account.as_deref(), Some("fixture-account"));
    assert_eq!(exchange.requestHeaders["authorization"], "[已脱敏]");
    assert!(!exchange.requestHeaders.to_string().contains(&token));
}

// 原生完成事件以 thread UUID 关联现有 active 绑定；只复制公开账号标识和显示名，不读取或保存令牌。
#[test]
fn clientCompletionUsesActiveSessionIdentity() {
    let mut storage = Storage::open_in_memory().expect("打开内存数据库");
    storage.init().expect("初始化内存数据库");
    storage
        .insert_account(&Account {
            id: "account-a".into(),
            label: "账号 A".into(),
            issuer: "fixture".into(),
            chatgpt_account_id: Some("workspace-a".into()),
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".into(),
            created_at: 1,
            updated_at: 1,
        })
        .expect("插入测试账号");
    storage
        .insert_token(&Token {
            account_id: "account-a".into(),
            id_token: String::new(),
            access_token: "fixture-access".into(),
            refresh_token: String::new(),
            api_key_access_token: None,
            last_refresh: 1,
        })
        .expect("插入测试令牌");
    storage
        .resolveSessionRouting("thread-a", "thread-id", 1)
        .expect("创建测试绑定");
    storage
        .set_app_setting(crate::sessionRouting::enabledSettingKey, "true", 1)
        .expect("启用测试分流");
    let mut request = RequestLog {
        request_type: Some(codexmanager_core::storage::observationClientRequestType.into()),
        route_strategy: Some("passthrough".into()),
        route_source: Some("thread-id".into()),
        actual_source_kind: Some("session".into()),
        actual_source_id: Some("thread-a".into()),
        ..Default::default()
    };

    enrichClientIdentity(&storage, &mut request).expect("补齐客户端完成身份");

    assert_eq!(request.account_id.as_deref(), Some("workspace-a"));
    assert_eq!(request.account_label.as_deref(), Some("账号 A"));
    assert_eq!(request.route_strategy.as_deref(), Some("sessionRouting"));
    assert_eq!(request.route_source.as_deref(), Some("thread-id"));

    storage
        .set_app_setting(crate::sessionRouting::enabledSettingKey, "false", 2)
        .expect("关闭测试分流");
    let mut passthrough = RequestLog {
        request_type: Some(codexmanager_core::storage::observationClientRequestType.into()),
        route_strategy: Some("passthrough".into()),
        route_source: Some("thread-id".into()),
        actual_source_kind: Some("session".into()),
        actual_source_id: Some("thread-a".into()),
        ..Default::default()
    };
    enrichClientIdentity(&storage, &mut passthrough).expect("保留关闭后的透传身份");
    assert!(passthrough.account_id.is_none());
    assert!(passthrough.account_label.is_none());
    assert_eq!(passthrough.route_strategy.as_deref(), Some("passthrough"));
}
