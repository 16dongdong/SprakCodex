use super::{
    build_warmup_headers, consume_warmup_stream, resolve_target_accounts,
    resolve_warmup_model_slug, should_retry_warmup_with_refresh, summarize_warmup_error,
    WarmupAuthorization, DEFAULT_WARMUP_MODEL,
};
use codexmanager_core::storage::{
    now_ts, Account, ManagedModelV2, ManagedModelV2Upsert, ModelPriceV2, Storage, Token,
};
use std::io::Cursor;

#[test]
fn summarize_warmup_error_redacts_invalid_agent_task_details() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-openai-authorization-error",
        reqwest::header::HeaderValue::from_static("task-secret-from-header"),
    );
    let summary = summarize_warmup_error(
        401,
        &headers,
        r#"{"error":{"code":"task_expired","message":"task-secret-from-body"}}"#,
    );

    assert!(summary.contains("invalid_task_id"));
    assert!(crate::agent_identity::is_agent_identity_task_invalid_error(
        &summary
    ));
    assert!(!summary.contains("task-secret-from-body"));
    assert!(!summary.contains("task-secret-from-header"));
}

fn make_model(slug: &str, sort_order: i64, supported_in_api: bool) -> ManagedModelV2Upsert {
    ManagedModelV2Upsert {
        model: ManagedModelV2 {
            slug: slug.to_string(),
            display_name: slug.to_string(),
            origin: "custom".to_string(),
            enabled: true,
            supported_in_api,
            visibility: "list".to_string(),
            sort_order,
            instructions_mode: "passthrough".to_string(),
            price: ModelPriceV2 {
                price_status: "missing".to_string(),
                ..Default::default()
            },
            ..ManagedModelV2::default()
        },
        ..ManagedModelV2Upsert::default()
    }
}

fn disable_seed_models(storage: &Storage) {
    for mut model in storage
        .list_managed_models_v2(true)
        .expect("list seeded models")
    {
        model.enabled = false;
        storage
            .upsert_managed_model_v2(&ManagedModelV2Upsert {
                model,
                ..Default::default()
            })
            .expect("disable seeded model");
    }
}

#[test]
fn resolve_warmup_model_slug_uses_first_supported_model_from_catalog_order() {
    let storage = Storage::open_in_memory().expect("open in-memory storage");
    storage.init().expect("init in-memory storage");
    disable_seed_models(&storage);
    let mut hidden = make_model("gpt-hidden", 0, true);
    hidden.model.visibility = "hide".to_string();
    let mut image = make_model("gpt-image-only", 0, true);
    image.model.capabilities = serde_json::json!({
        "supports_text_generation": false,
        "output_modalities": ["image"]
    });
    for model in [
        hidden,
        image,
        make_model("gpt-unsupported", 1, false),
        make_model("gpt-latest", 1, true),
        make_model("gpt-older", 2, true),
    ] {
        storage
            .upsert_managed_model_v2(&model)
            .expect("save model catalog V2 item");
    }

    assert_eq!(resolve_warmup_model_slug(&storage), "gpt-latest");
}

#[test]
fn resolve_warmup_model_slug_falls_back_when_catalog_missing() {
    let storage = Storage::open_in_memory().expect("open in-memory storage");
    storage.init().expect("init in-memory storage");
    disable_seed_models(&storage);
    assert_eq!(resolve_warmup_model_slug(&storage), DEFAULT_WARMUP_MODEL);
}

#[test]
fn should_retry_warmup_with_refresh_only_for_auth_errors_with_refresh_token() {
    let mut token = Token {
        account_id: "account-1".to_string(),
        id_token: String::new(),
        access_token: String::new(),
        refresh_token: "refresh-token".to_string(),
        api_key_access_token: None,
        last_refresh: 0,
    };

    assert!(should_retry_warmup_with_refresh(
        &token,
        "status=401 body=Unauthorized"
    ));
    assert!(!should_retry_warmup_with_refresh(
        &token,
        "status=500 body=server error"
    ));

    token.refresh_token.clear();
    assert!(!should_retry_warmup_with_refresh(
        &token,
        "status=401 body=Unauthorized"
    ));
}

#[test]
fn resolve_target_accounts_only_returns_gateway_available_accounts() {
    let storage = Storage::open_in_memory().expect("open in-memory storage");
    storage.init().expect("init in-memory storage");
    let now = now_ts();

    for (id, status) in [
        ("acc-active", "active"),
        ("acc-unavailable", "unavailable"),
        ("acc-disabled", "disabled"),
        ("acc-banned", "banned"),
        ("acc-inactive", "inactive"),
    ] {
        storage
            .insert_account(&Account {
                id: id.to_string(),
                label: id.to_string(),
                issuer: "issuer".to_string(),
                chatgpt_account_id: None,
                workspace_id: None,
                group_name: None,
                sort: 0,
                status: status.to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: id.to_string(),
                id_token: "id-token".to_string(),
                access_token: "access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                api_key_access_token: None,
                last_refresh: now,
            })
            .expect("insert token");
    }

    let all_targets = resolve_target_accounts(&storage, &[]).expect("resolve all targets");
    assert_eq!(all_targets.len(), 1);
    assert_eq!(all_targets[0].account.id, "acc-active");
    assert_eq!(all_targets[0].token.account_id, "acc-active");

    let selected_targets = resolve_target_accounts(
        &storage,
        &[
            "acc-unavailable".to_string(),
            "acc-active".to_string(),
            "acc-disabled".to_string(),
        ],
    )
    .expect("resolve selected targets");
    assert_eq!(selected_targets.len(), 1);
    assert_eq!(selected_targets[0].account.id, "acc-active");
    assert_eq!(selected_targets[0].token.account_id, "acc-active");
}

#[test]
fn build_warmup_headers_omits_non_codex_headers() {
    let account = Account {
        id: "acc-1".to_string(),
        label: "acc-1".to_string(),
        issuer: "issuer".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let authorization = WarmupAuthorization {
        value: "bearer-token".to_string(),
        task_id: None,
        is_fedramp: false,
        uses_agent_identity: false,
        account_scope_id: None,
    };
    let headers = build_warmup_headers(&account, &authorization).expect("build warmup headers");

    assert!(headers.get("version").is_none());
    assert!(headers.get("openai-organization").is_none());
    assert!(headers.get("openai-project").is_none());
    assert!(headers.get("client_version").is_none());
    assert_eq!(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer bearer-token")
    );
    assert!(headers.get("x-openai-fedramp").is_none());
}

#[test]
fn build_warmup_headers_preserves_agent_assertion_and_fedramp_context() {
    let account = Account {
        id: "acc-agent".to_string(),
        label: "acc-agent".to_string(),
        issuer: "issuer".to_string(),
        chatgpt_account_id: Some("workspace-agent".to_string()),
        workspace_id: Some("workspace-agent".to_string()),
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let authorization = WarmupAuthorization {
        value: "AgentAssertion encoded".to_string(),
        task_id: Some("task-agent".to_string()),
        is_fedramp: true,
        uses_agent_identity: true,
        account_scope_id: Some("agent-bound-scope".to_string()),
    };
    let headers =
        build_warmup_headers(&account, &authorization).expect("build agent warmup headers");

    assert_eq!(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("AgentAssertion encoded")
    );
    assert_eq!(
        headers
            .get("chatgpt-account-id")
            .and_then(|value| value.to_str().ok()),
        Some("agent-bound-scope")
    );
    assert_eq!(
        headers
            .get("x-openai-fedramp")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
}

#[test]
fn consume_warmup_stream_waits_for_response_completed() {
    let stream = Cursor::new(
        "event: response.created\n\
         data: {\"type\":\"response.created\"}\n\n\
         event: response.completed\n\
         data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n",
    );

    assert!(consume_warmup_stream(stream).is_ok());
}

#[test]
fn consume_warmup_stream_rejects_incomplete_stream() {
    let stream = Cursor::new(
        "event: response.created\n\
         data: {\"type\":\"response.created\"}\n\n",
    );

    let err = consume_warmup_stream(stream).expect_err("stream should be incomplete");
    assert!(err.contains("before response.completed"));
}

#[test]
fn consume_warmup_stream_reports_error_event() {
    let stream = Cursor::new(
        "event: response.failed\n\
         data: {\"type\":\"response.failed\",\"error\":{\"message\":\"quota exceeded\"}}\n\n",
    );

    let err = consume_warmup_stream(stream).expect_err("stream should fail");
    assert!(err.contains("quota exceeded"));
}
