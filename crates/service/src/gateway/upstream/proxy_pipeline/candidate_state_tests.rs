use super::CandidateExecutionState;
use bytes::Bytes;
use codexmanager_core::storage::Account;

fn sample_setup() -> super::super::request_setup::UpstreamRequestSetup {
    super::super::request_setup::UpstreamRequestSetup {
        upstream_base: "https://chatgpt.com/backend-api/codex".to_string(),
        upstream_fallback_base: None,
        url: "https://chatgpt.com/backend-api/codex/responses".to_string(),
        url_alt: None,
        candidate_count: 1,
        account_max_inflight: 1,
        anthropic_has_thread_anchor: false,
        has_sticky_fallback_session: false,
        has_sticky_fallback_conversation: false,
        has_body_encrypted_content: false,
        conversation_routing: None,
        route_strategy_for_log: "ordered",
        route_source_for_log: "route_strategy",
    }
}

/// 函数 `body_for_attempt_rewrites_model_override`
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
fn body_for_attempt_rewrites_model_override() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(br#"{"model":"gpt-5.4","input":"hello"}"#);
    let setup = sample_setup();

    let actual = state.body_for_attempt(
        "/v1/responses",
        &body,
        false,
        &setup,
        Some("gpt-5.2"),
        Some("thread-2"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(
        value.get("model").and_then(serde_json::Value::as_str),
        Some("gpt-5.2")
    );
    assert_eq!(
        value
            .get("prompt_cache_key")
            .and_then(serde_json::Value::as_str),
        None
    );
}

#[test]
fn body_for_account_attempt_preserves_explicit_reserve_alias() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(br#"{"model":"gpt-reserve","input":"hello"}"#);
    let setup = sample_setup();

    let actual = state.body_for_attempt(
        "/v1/responses",
        &body,
        false,
        &setup,
        Some("gpt-5.6-luna"),
        None,
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(value["model"], "gpt-reserve");
}

#[test]
fn body_for_account_attempt_keeps_non_luna_model_binding_authoritative() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(br#"{"model":"gpt-reserve","input":"hello"}"#);
    let setup = sample_setup();

    let actual =
        state.body_for_attempt("/v1/responses", &body, false, &setup, Some("gpt-5.4"), None);
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(value["model"], "gpt-5.4");
}

#[test]
fn body_for_attempt_keeps_native_codex_retry_shape() {
    let _guard = crate::test_env_guard();
    let mut state = CandidateExecutionState::default();
    let body =
        Bytes::from_static(br#"{"model":"gpt-5.4","input":"hello","stream":false,"store":true}"#);
    let setup = sample_setup();

    let actual = state.body_for_attempt(
        "/v1/responses",
        &body,
        false,
        &setup,
        Some("gpt-5.2"),
        Some("thread-2"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(
        value.get("model").and_then(serde_json::Value::as_str),
        Some("gpt-5.2")
    );
    assert_eq!(
        value.get("stream").and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        value.get("store").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(value.get("prompt_cache_key").is_none());
    assert_eq!(
        value
            .get("instructions")
            .and_then(serde_json::Value::as_str),
        Some("Follow the user's instructions.")
    );
    assert!(value.get("tool_choice").is_none());
    assert!(value.get("include").is_none());
}

#[test]
fn body_for_attempt_injects_local_thread_anchor_without_compat_shape() {
    let mut state = CandidateExecutionState::default();
    let body =
        Bytes::from_static(br#"{"model":"gpt-5.4","input":"hello","stream":false,"store":true}"#);
    let mut setup = sample_setup();
    setup.has_sticky_fallback_conversation = true;

    let actual = state.body_for_attempt(
        "/v1/responses",
        &body,
        false,
        &setup,
        Some("gpt-5.2"),
        Some("thread-2"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(
        value.get("model").and_then(serde_json::Value::as_str),
        Some("gpt-5.2")
    );
    assert_eq!(
        value
            .get("prompt_cache_key")
            .and_then(serde_json::Value::as_str),
        Some("thread-2")
    );
    assert_eq!(
        value.get("stream").and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        value.get("store").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        value
            .get("instructions")
            .and_then(serde_json::Value::as_str),
        Some("Follow the user's instructions.")
    );
    assert!(value.get("tool_choice").is_none());
    assert!(value.get("include").is_none());
}

#[test]
fn body_for_attempt_preserves_existing_prompt_cache_key() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(
        br#"{"model":"gpt-5.4","input":"hello","prompt_cache_key":"client-thread"}"#,
    );
    let setup = sample_setup();

    let actual = state.body_for_attempt(
        "/v1/responses",
        &body,
        false,
        &setup,
        None,
        Some("thread-from-conversation"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(
        value
            .get("prompt_cache_key")
            .and_then(serde_json::Value::as_str),
        Some("client-thread")
    );
}

#[test]
fn compact_body_for_attempt_preserves_existing_prompt_cache_key() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(
        br#"{"model":"gpt-5.5","input":"hello","prompt_cache_key":"client-thread"}"#,
    );
    let mut setup = sample_setup();
    setup.has_sticky_fallback_conversation = true;

    let actual = state.body_for_attempt(
        "/v1/responses/compact",
        &body,
        true,
        &setup,
        None,
        Some("thread-from-conversation"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse rewritten body");

    assert_eq!(
        value
            .get("prompt_cache_key")
            .and_then(serde_json::Value::as_str),
        Some("client-thread")
    );
}

#[test]
fn stripped_candidate_removes_account_scoped_items_without_leaving_invalid_shells() {
    let mut state = CandidateExecutionState::default();
    let body = Bytes::from_static(
        br#"{
            "model":"gpt-5.6-sol",
            "input":[
                {
                    "type":"reasoning",
                    "id":"rs_1",
                    "summary":[],
                    "encrypted_content":"reasoning-secret"
                },
                {
                    "type":"compaction",
                    "id":"cmp_1",
                    "encrypted_content":"compaction-secret"
                },
                {
                    "type":"agent_message",
                    "content":[
                        {"type":"input_text","text":"keep me"},
                        {"type":"encrypted_content","encrypted_content":"nested-secret"}
                    ]
                },
                {
                    "type":"message",
                    "role":"user",
                    "content":[{"type":"input_text","text":"continue"}]
                }
            ]
        }"#,
    );
    let mut setup = sample_setup();
    setup.has_body_encrypted_content = true;

    let actual = state.body_for_attempt("/v1/responses", &body, true, &setup, None, None);
    let value: serde_json::Value =
        serde_json::from_slice(actual.as_ref()).expect("parse stripped candidate body");
    let input = value["input"].as_array().expect("input array");

    assert_eq!(input.len(), 2, "reasoning item must be removed");
    assert_eq!(input[0]["type"], "agent_message");
    assert_eq!(
        input[0]["content"],
        serde_json::json!([{"type":"input_text","text":"keep me"}])
    );
    assert_eq!(input[1]["type"], "message");
}

#[test]
fn strip_session_affinity_preserves_same_workspace_when_thread_anchor_exists() {
    let mut state = CandidateExecutionState::default();
    let first = Account {
        id: "acc-1".to_string(),
        label: "acc-1".to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: Some("ws-same".to_string()),
        group_name: None,
        sort: 1,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let second = Account {
        id: "acc-2".to_string(),
        label: "acc-2".to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: Some("ws-same".to_string()),
        group_name: None,
        sort: 2,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    assert!(!state.strip_session_affinity(&first, 0, true));
    assert!(!state.strip_session_affinity(&second, 1, true));
}

#[test]
fn strip_session_affinity_strips_cross_workspace_when_thread_anchor_exists() {
    let mut state = CandidateExecutionState::default();
    let first = Account {
        id: "acc-1".to_string(),
        label: "acc-1".to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: Some("ws-a".to_string()),
        group_name: None,
        sort: 1,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let second = Account {
        id: "acc-2".to_string(),
        label: "acc-2".to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: Some("ws-b".to_string()),
        group_name: None,
        sort: 2,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    assert!(!state.strip_session_affinity(&first, 0, true));
    assert!(state.strip_session_affinity(&second, 1, true));
}
