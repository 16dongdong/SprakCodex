use super::*;
use codexmanager_core::storage::AggregateApi;
use codexmanager_core::storage::RequestTokenStat;

const MISSING_AUTH_JSON_OPENAI_API_KEY_ERROR: &str =
    "配置错误：未配置auth.json的OPENAI_API_KEY(invalid api key)";
const LEGACY_COMPACT_MODEL_FORWARD_RULES_SETTING_KEY: &str = "gateway.compact_model_forward_rules";

/// 函数 `gateway_logs_invalid_api_key_error`
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
fn gateway_logs_invalid_api_key_error() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
        ],
    );
    assert_eq!(status, 403);
    assert!(
        body.contains("invalid api key"),
        "gateway should return raw upstream message, got {body}"
    );
    assert!(
        !body.contains("未配置auth.json"),
        "gateway response should not expose bilingual log text, got {body}"
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let mut logs = Vec::new();
    for _ in 0..40 {
        logs = storage
            .list_request_logs(None, 100)
            .expect("list request logs");
        if !logs.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let found = logs.iter().any(|item| {
        item.request_path == "/v1/responses"
            && item.status_code == Some(403)
            && item.input_tokens.is_none()
            && item.cached_input_tokens.is_none()
            && item.output_tokens.is_none()
            && item.total_tokens.is_none()
            && item.reasoning_output_tokens.is_none()
            && item.error.as_deref() == Some(MISSING_AUTH_JSON_OPENAI_API_KEY_ERROR)
    });
    assert!(
        found,
        "expected missing auth.json OPENAI_API_KEY request to be logged, got {:?}",
        logs.iter()
            .map(|v| (&v.request_path, v.status_code, v.error.as_deref()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn gateway_rejects_api_key_after_quota_limit() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-key-quota");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let platform_key = "pk_quota_limit_reached";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    storage
        .insert_api_key(&ApiKey {
            id: "gk_quota_limit_reached".to_string(),
            name: Some("quota-limit".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
    storage
        .upsert_api_key_quota_limit("gk_quota_limit_reached", Some(100))
        .expect("upsert quota");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id: 1,
            key_id: Some("gk_quota_limit_reached".to_string()),
            total_tokens: Some(100),
            created_at: now,
            ..RequestTokenStat::default()
        })
        .expect("insert token stat");

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    assert_eq!(status, 429, "response body: {body}");
    assert!(
        body.contains("quota") || body.contains("额度"),
        "gateway should report quota exhaustion, got {body}"
    );
}

#[test]
fn gateway_reports_wallet_quota_exhaustion_in_chinese() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-wallet-quota");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let platform_key = "pk_wallet_quota_exhausted";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    storage
        .set_app_setting("distribution.enabled", "true", now)
        .expect("enable distribution");
    storage
        .insert_app_user(&AppUser {
            id: "usr_wallet_quota_exhausted".to_string(),
            username: "wallet-quota-member".to_string(),
            display_name: None,
            password_hash: "test-password-hash".to_string(),
            role: "member".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
        })
        .expect("insert app user");
    storage
        .insert_api_key(&ApiKey {
            id: "gk_wallet_quota_exhausted".to_string(),
            name: Some("wallet-quota".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
    storage
        .upsert_api_key_owner(&ApiKeyOwner {
            key_id: "gk_wallet_quota_exhausted".to_string(),
            owner_kind: "user".to_string(),
            owner_user_id: Some("usr_wallet_quota_exhausted".to_string()),
            project_id: None,
            updated_at: now,
        })
        .expect("insert api key owner");
    storage
        .ensure_wallet_for_owner(
            "wlt_wallet_quota_exhausted",
            "user",
            "usr_wallet_quota_exhausted",
        )
        .expect("ensure zero wallet");
    storage
        .replace_user_model_groups_for_group(
            "mg_default",
            &[UserModelGroup {
                user_id: "usr_wallet_quota_exhausted".to_string(),
                group_id: "mg_default".to_string(),
                status: "active".to_string(),
                expires_at: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("assign default model group");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    assert_eq!(status, 402, "response body: {body}");
    assert!(
        body.contains("额度不足，请联系管理员"),
        "gateway should report wallet quota exhaustion in Chinese, got {body}"
    );
    assert!(
        !body.contains("Too Many Requests"),
        "wallet quota exhaustion must not look like retryable upstream throttling, got {body}"
    );
}

#[test]
fn gateway_reports_platform_model_route_errors() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-model-route-errors");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let platform_key = "pk_model_route_errors";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    storage
        .insert_api_key(&ApiKey {
            id: "gk_model_route_errors".to_string(),
            name: Some("model-route-errors".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        r#"{"model":"missing-platform","input":"hello"}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 404, "response body: {body}");
    assert!(
        body.contains("model_not_found"),
        "gateway should report missing platform model, got {body}"
    );

    seed_model_catalog_models(&storage, &["gpt-platform"]);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        r#"{"model":"gpt-platform","input":"hello"}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 503, "response body: {body}");
    assert!(
        body.contains("no_available_account"),
        "gateway should report a routed model without available account candidates, got {body}"
    );
}

#[test]
fn gateway_follows_client_model_even_when_account_source_mapping_exists() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-account-model-mapping");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let response = serde_json::json!({
        "id": "resp_model_follow",
        "model": "gpt-platform",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    });
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_once(&serde_json::to_string(&response).expect("serialize response"));
    let upstream_base = format!("http://{upstream_addr}/v1");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let platform_key = "pk_account_model_mapping";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    seed_model_catalog_models(&storage, &["gpt-platform"]);
    storage
        .insert_account(&Account {
            id: "acc_model_mapping".to_string(),
            label: "mapping account".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: None,
            workspace_id: Some("ws_model_mapping".to_string()),
            group_name: None,
            sort: 1,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_model_mapping".to_string(),
            id_token: String::new(),
            access_token: "access_token_model_mapping".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_token_model_mapping".to_string()),
            last_refresh: now,
        })
        .expect("insert token");
    storage
        .insert_api_key(&ApiKey {
            id: "gk_account_model_mapping".to_string(),
            name: Some("account-model-mapping".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "openai_account".to_string(),
            source_id: "acc_model_mapping".to_string(),
            upstream_model: "gpt-platform".to_string(),
            display_name: None,
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("upsert source model");
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "map_account_model_mapping".to_string(),
            platform_model_slug: "gpt-platform".to_string(),
            source_kind: "openai_account".to_string(),
            source_id: "acc_model_mapping".to_string(),
            upstream_model: "gpt-upstream".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("upsert mapping");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        r#"{"model":"gpt-platform","input":"hello","stream":false}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let upstream_request = upstream_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive upstream request");
    upstream_join.join().expect("join upstream");
    let request_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&upstream_request))
            .expect("parse upstream request body");
    assert_eq!(
        request_body
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-platform")
    );
    let logs = storage
        .list_request_logs(None, 10)
        .expect("list request logs");
    let log = logs.first().expect("request log should be written");
    assert_eq!(log.model.as_deref(), Some("gpt-platform"));
    assert_eq!(log.upstream_model.as_deref(), None);
    assert_eq!(log.actual_source_kind.as_deref(), Some("openai_account"));
    assert_eq!(log.actual_source_id.as_deref(), Some("acc_model_mapping"));
}

#[test]
fn gateway_applies_saved_model_forward_rules_to_codex_responses_request() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-saved-model-forward-rules");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let response = serde_json::json!({
        "id": "resp_saved_model_forward_rules",
        "model": "gpt-5.4-mini",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    });
    let platform_key = "pk_saved_model_forward_rules";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let _rules_guard = GatewayModelForwardRulesResetGuard::reset();
    let now = now_ts();
    seed_model_catalog_models(&storage, &["spark", "gpt-5.4-mini"]);
    storage
        .insert_account(&Account {
            id: "acc_saved_model_forward_rules".to_string(),
            label: "saved-model-forward-rules".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: None,
            workspace_id: Some("ws_saved_model_forward_rules".to_string()),
            group_name: None,
            sort: 1,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_saved_model_forward_rules".to_string(),
            id_token: String::new(),
            access_token: "access_token_saved_model_forward_rules".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_token_saved_model_forward_rules".to_string()),
            last_refresh: now,
        })
        .expect("insert token");
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "openai_account".to_string(),
            source_id: "acc_saved_model_forward_rules".to_string(),
            upstream_model: "gpt-5.4-mini".to_string(),
            display_name: Some("gpt-5.4-mini".to_string()),
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("seed source model");
    storage
        .insert_api_key(&ApiKey {
            id: "gk_saved_model_forward_rules".to_string(),
            name: Some("saved-model-forward-rules".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    codexmanager_service::app_settings_set(Some(&serde_json::json!({
        "modelForwardRules": "spark*=gpt-5.4-mini"
    })))
    .expect("save app settings");

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_once(&serde_json::to_string(&response).expect("serialize response"));
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        r#"{"model":"spark","input":"hello","stream":false}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let upstream_request = upstream_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive upstream request");
    upstream_join.join().expect("join upstream");
    let request_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&upstream_request))
            .expect("parse upstream request body");
    assert_eq!(
        request_body
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-5.4-mini")
    );
}

#[test]
fn gateway_aggregate_api_model_override_rewrites_minimax_responses_request() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-aggregate-minimax-model-override");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let response = serde_json::json!({
        "id": "resp_minimax_model_override",
        "model": "MiniMax-M3",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    });
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_once(&serde_json::to_string(&response).expect("serialize response"));

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    let aggregate_id = "agg_minimax_model_override";
    seed_model_catalog_models(&storage, &["gpt-5.4", "MiniMax-M3"]);
    storage
        .insert_aggregate_api(&AggregateApi {
            id: aggregate_id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some("Minimax".to_string()),
            sort: 5,
            url: format!("http://{upstream_addr}/v1"),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: Some("MiniMax-M3".to_string()),
            user_agent: None,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            balance_query_enabled: false,
            balance_query_template: None,
            balance_query_base_url: None,
            balance_query_user_id: None,
            balance_query_config_json: None,
            last_balance_at: None,
            last_balance_status: None,
            last_balance_error: None,
            last_balance_json: None,
        })
        .expect("insert aggregate api");
    storage
        .upsert_aggregate_api_secret(aggregate_id, "upstream-secret")
        .expect("insert aggregate secret");
    seed_model_catalog_route(
        &storage,
        "gpt-5.4",
        "aggregate_api",
        aggregate_id,
        "MiniMax-M3",
        10,
    );
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "aggregate_api".to_string(),
            source_id: aggregate_id.to_string(),
            upstream_model: "MiniMax-M3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert aggregate source model");
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping_minimax_model_override".to_string(),
            platform_model_slug: "MiniMax-M3".to_string(),
            source_kind: "aggregate_api".to_string(),
            source_id: aggregate_id.to_string(),
            upstream_model: "MiniMax-M3".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("insert aggregate source mapping");

    let platform_key = "pk_minimax_model_override";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_minimax_model_override".to_string(),
            name: Some("minimax-model-override".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "aggregate_api_rotation".to_string(),
            aggregate_api_id: Some(aggregate_id.to_string()),
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request = serde_json::json!({
        "model": "gpt-5.4",
        "input": [
            {
                "type": "reasoning",
                "summary": []
            },
            {
                "role": "user",
                "content": [{ "type": "input_text", "text": "hello" }]
            }
        ],
        "stream": false
    });
    let request = serde_json::to_string(&request).expect("serialize request");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive upstream request");
    upstream_join.join().expect("join upstream");
    assert_eq!(captured.path, "/v1/responses");
    let request_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&captured))
            .expect("parse upstream request body");
    assert_eq!(
        request_body
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("MiniMax-M3")
    );
    assert_eq!(
        request_body
            .get("input")
            .and_then(serde_json::Value::as_str),
        Some("hello")
    );
    assert_eq!(
        request_body
            .get("stream")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(request_body.get("instructions").is_none());
    assert!(request_body.get("store").is_none());
    assert!(request_body.get("tool_choice").is_none());
    assert!(request_body.get("include").is_none());

    let log = storage
        .list_request_logs(Some("key:=gk_minimax_model_override"), 10)
        .expect("list request logs")
        .into_iter()
        .find(|item| item.request_path == "/v1/responses")
        .expect("request log");
    assert_eq!(log.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(log.upstream_model.as_deref(), Some("MiniMax-M3"));
}

#[test]
fn gateway_aggregate_codex_failover_to_minimax_isolates_candidate_request_bodies() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-aggregate-codex-minimax-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let retryable_error = serde_json::json!({
        "error": {
            "message": "temporary upstream failure",
            "type": "server_error"
        }
    });
    let retryable_error =
        serde_json::to_string(&retryable_error).expect("serialize retryable response");
    let (codex_addr, codex_rx, codex_join) = start_mock_upstream_sequence(vec![
        (500, retryable_error.clone()),
        (500, retryable_error.clone()),
        (500, retryable_error.clone()),
        (500, retryable_error),
    ]);

    let success_response = serde_json::json!({
        "id": "resp_aggregate_failover_minimax",
        "model": "MiniMax-M3",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    });
    let (minimax_addr, minimax_rx, minimax_join) = start_mock_upstream_once(
        &serde_json::to_string(&success_response).expect("serialize success response"),
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    let codex_id = "agg_failover_codex";
    let minimax_id = "agg_failover_minimax";
    seed_model_catalog_models(&storage, &["gpt-5.4", "MiniMax-M3"]);

    let codex_candidate = AggregateApi {
        id: codex_id.to_string(),
        provider_type: "codex".to_string(),
        supplier_name: Some("OpenAI Codex".to_string()),
        sort: 10,
        url: format!("http://{codex_addr}/backend-api/codex"),
        auth_type: "apikey".to_string(),
        auth_params_json: None,
        action: Some("/responses".to_string()),
        model_override: Some("gpt-5.4".to_string()),
        user_agent: None,
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
        last_test_at: None,
        last_test_status: None,
        last_test_error: None,
        balance_query_enabled: false,
        balance_query_template: None,
        balance_query_base_url: None,
        balance_query_user_id: None,
        balance_query_config_json: None,
        last_balance_at: None,
        last_balance_status: None,
        last_balance_error: None,
        last_balance_json: None,
    };
    storage
        .insert_aggregate_api(&codex_candidate)
        .expect("insert Codex aggregate API");

    let mut minimax_candidate = codex_candidate.clone();
    minimax_candidate.id = minimax_id.to_string();
    minimax_candidate.supplier_name = Some("MiniMax".to_string());
    minimax_candidate.sort = 20;
    minimax_candidate.url = format!("http://{minimax_addr}/v1");
    minimax_candidate.action = None;
    minimax_candidate.model_override = Some("MiniMax-M3".to_string());
    storage
        .insert_aggregate_api(&minimax_candidate)
        .expect("insert MiniMax aggregate API");

    for aggregate_id in [codex_id, minimax_id] {
        storage
            .upsert_aggregate_api_secret(aggregate_id, "upstream-secret")
            .expect("insert aggregate secret");
    }
    seed_model_catalog_route(
        &storage,
        "gpt-5.4",
        "aggregate_api",
        codex_id,
        "gpt-5.4",
        20,
    );
    seed_model_catalog_route(
        &storage,
        "gpt-5.4",
        "aggregate_api",
        minimax_id,
        "MiniMax-M3",
        10,
    );

    let platform_key = "pk_aggregate_codex_minimax_failover";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_aggregate_codex_minimax_failover".to_string(),
            name: Some("aggregate-codex-minimax-failover".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "aggregate_api_rotation".to_string(),
            aggregate_api_id: Some(codex_id.to_string()),
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request = serde_json::json!({
        "model": "gpt-5.4",
        "input": [{
            "role": "user",
            "content": [{ "type": "input_text", "text": "hello" }]
        }],
        "stream": false
    });
    let request = serde_json::to_string(&request).expect("serialize request");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let mut codex_bodies = Vec::new();
    for attempt in 0..4 {
        let captured = codex_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("receive Codex upstream request attempt {attempt}"));
        assert_eq!(captured.path, "/backend-api/codex/responses");
        let body: serde_json::Value =
            serde_json::from_slice(&decode_upstream_request_body(&captured))
                .expect("parse Codex upstream request body");
        codex_bodies.push(body);
    }
    codex_join.join().expect("join Codex upstream");

    let first_codex_body = codex_bodies.first().expect("first Codex request body");
    assert!(
        codex_bodies.iter().all(|body| body == first_codex_body),
        "Codex retries must reuse the same candidate-scoped request body"
    );
    assert_eq!(
        first_codex_body
            .get("instructions")
            .and_then(serde_json::Value::as_str),
        Some("Follow the user's instructions.")
    );
    assert_eq!(
        first_codex_body
            .get("stream")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        first_codex_body
            .get("store")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        first_codex_body
            .get("tool_choice")
            .and_then(serde_json::Value::as_str),
        Some("auto")
    );
    assert!(first_codex_body
        .get("include")
        .is_some_and(serde_json::Value::is_array));
    assert!(first_codex_body
        .get("input")
        .is_some_and(serde_json::Value::is_array));

    let minimax_request = minimax_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive MiniMax upstream request");
    minimax_join.join().expect("join MiniMax upstream");
    assert_eq!(minimax_request.path, "/v1/responses");
    let minimax_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&minimax_request))
            .expect("parse MiniMax upstream request body");
    assert_eq!(
        minimax_body
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("MiniMax-M3")
    );
    assert_eq!(
        minimax_body
            .get("input")
            .and_then(serde_json::Value::as_str),
        Some("hello")
    );
    assert_eq!(
        minimax_body
            .get("stream")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    for codex_only_field in ["instructions", "store", "tool_choice", "include"] {
        assert!(
            minimax_body.get(codex_only_field).is_none(),
            "MiniMax failover body inherited Codex-only field {codex_only_field}: {minimax_body}"
        );
    }
}

#[test]
fn gateway_applies_migrated_legacy_compact_model_forward_rules() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-legacy-compact-forward-rules");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let response = serde_json::json!({
        "id": "resp_legacy_compact_forward_rules",
        "model": "gpt-5.4-openai-compact",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    });
    let platform_key = "pk_legacy_compact_forward_rules";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let _rules_guard = GatewayModelForwardRulesResetGuard::reset();
    let now = now_ts();
    seed_model_catalog_models(&storage, &["gpt-5.4", "gpt-5.4-openai-compact"]);
    storage
        .insert_account(&Account {
            id: "acc_legacy_compact_forward_rules".to_string(),
            label: "legacy-compact-forward-rules".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: None,
            workspace_id: Some("ws_legacy_compact_forward_rules".to_string()),
            group_name: None,
            sort: 1,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_legacy_compact_forward_rules".to_string(),
            id_token: String::new(),
            access_token: "access_token_legacy_compact_forward_rules".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_token_legacy_compact_forward_rules".to_string()),
            last_refresh: now,
        })
        .expect("insert token");
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "openai_account".to_string(),
            source_id: "acc_legacy_compact_forward_rules".to_string(),
            upstream_model: "gpt-5.4-openai-compact".to_string(),
            display_name: Some("gpt-5.4-openai-compact".to_string()),
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("seed source model");
    storage
        .set_app_setting(
            LEGACY_COMPACT_MODEL_FORWARD_RULES_SETTING_KEY,
            "gpt-5.4=gpt-5.4-openai-compact",
            now,
        )
        .expect("save legacy compact model forward rules");
    storage
        .insert_api_key(&ApiKey {
            id: "gk_legacy_compact_forward_rules".to_string(),
            name: Some("legacy-compact-forward-rules".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
    drop(storage);

    codexmanager_service::sync_runtime_settings_from_storage();
    assert_eq!(
        codexmanager_service::current_gateway_model_forward_rules(),
        "gpt-5.4=gpt-5.4-openai-compact"
    );

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_once(&serde_json::to_string(&response).expect("serialize response"));
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses/compact",
        r#"{"model":"gpt-5.4","input":"hello","stream":false}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let upstream_request = upstream_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive upstream request");
    upstream_join.join().expect("join upstream");
    let request_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&upstream_request))
            .expect("parse upstream request body");
    assert_eq!(
        request_body
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-5.4-openai-compact")
    );
}

/// 函数 `gateway_tolerates_non_ascii_turn_metadata_header`
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
fn gateway_tolerates_non_ascii_turn_metadata_header() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs-nonascii");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let metadata = r#"{"workspaces":{"D:\\MyComputer\\own\\GPTTeam相关\\CodexManager\\CodexManager":{"latest_git_commit_hash":"abc123"}}}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
            ("x-codex-turn-metadata", metadata),
        ],
    );
    assert_eq!(status, 403, "response body: {body}");
}
