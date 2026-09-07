use super::{
    account_with_authorization_scope, resolve_chatgpt_primary_bearer, PrimaryAuthorization,
};
use codexmanager_core::storage::{Account, Token};

/// 函数 `build_token`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - access_token: 参数 access_token
///
/// # 返回
/// 返回函数执行结果
fn build_token(access_token: &str) -> Token {
    Token {
        account_id: "acc-test".to_string(),
        id_token: "id-token".to_string(),
        access_token: access_token.to_string(),
        refresh_token: "refresh-token".to_string(),
        api_key_access_token: Some("api-key-token".to_string()),
        last_refresh: 0,
    }
}

/// 函数 `chatgpt_primary_bearer_prefers_access_token`
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
fn chatgpt_primary_bearer_prefers_access_token() {
    let token = build_token("access-token");
    assert_eq!(
        resolve_chatgpt_primary_bearer(&token).as_deref(),
        Some("access-token")
    );
}

/// 函数 `chatgpt_primary_bearer_rejects_empty_access_token`
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
fn chatgpt_primary_bearer_rejects_empty_access_token() {
    let token = build_token("   ");
    assert!(resolve_chatgpt_primary_bearer(&token).is_none());
}

#[test]
fn agent_identity_scope_overrides_stale_account_headers() {
    let account = Account {
        id: "acc-test".to_string(),
        label: "test".to_string(),
        issuer: "issuer".to_string(),
        chatgpt_account_id: Some("stale-chatgpt".to_string()),
        workspace_id: Some("stale-workspace".to_string()),
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let authorization = PrimaryAuthorization {
        value: "AgentAssertion encoded".to_string(),
        task_id: Some("task-1".to_string()),
        uses_agent_identity: true,
        is_fedramp: false,
        account_scope_id: Some("agent-bound-scope".to_string()),
    };

    let scoped = account_with_authorization_scope(&account, &authorization);
    assert_eq!(
        scoped.chatgpt_account_id.as_deref(),
        Some("agent-bound-scope")
    );
    assert_eq!(scoped.workspace_id.as_deref(), Some("agent-bound-scope"));
    assert_eq!(account.workspace_id.as_deref(), Some("stale-workspace"));
}
