use super::*;
use codexmanager_core::storage::ModelFastPolicyV2;

#[test]
fn gateway_applies_model_fast_policy_to_forwarded_requests() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-model-fast-policy");
    let db_path = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let upstream_response = serde_json::json!({
        "id": "resp_model_fast_policy",
        "model": "gpt-fast-policy",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "ok" }]
        }],
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    })
    .to_string();
    let (upstream_addr, upstream_rx, upstream_join) = start_mock_upstream_sequence(vec![
        (200, upstream_response.clone()),
        (200, upstream_response.clone()),
        (200, upstream_response.clone()),
        (200, upstream_response),
    ]);
    let upstream_base = format!("http://{upstream_addr}/v1");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let model_slug = "gpt-fast-policy";
    let platform_key = "pk_model_fast_policy";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    seed_model_catalog_models(&storage, &[model_slug]);
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: "acc_model_fast_policy".to_string(),
            label: "model fast policy account".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: None,
            workspace_id: Some("ws_model_fast_policy".to_string()),
            group_name: None,
            sort: 1,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_model_fast_policy".to_string(),
            id_token: String::new(),
            access_token: "access_token_model_fast_policy".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_token_model_fast_policy".to_string()),
            last_refresh: now,
        })
        .expect("insert token");
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "openai_account".to_string(),
            source_id: "acc_model_fast_policy".to_string(),
            upstream_model: model_slug.to_string(),
            display_name: Some(model_slug.to_string()),
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert source model");
    storage
        .insert_api_key(&ApiKey {
            id: "gk_model_fast_policy".to_string(),
            name: Some("model-fast-policy".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: Some("fast".to_string()),
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

    let forwarded_cases = [
        (
            ModelFastPolicyV2::Passthrough,
            Some("fast"),
            Some("priority"),
        ),
        (ModelFastPolicyV2::Filter, None, None),
        (ModelFastPolicyV2::Force, None, Some("priority")),
        (ModelFastPolicyV2::Block, None, Some("priority")),
    ];
    for (policy, client_tier, expected_upstream_tier) in forwarded_cases {
        let mut model = storage
            .get_managed_model_v2(model_slug)
            .expect("get model")
            .expect("model exists");
        model.fast_policy = policy;
        storage
            .upsert_managed_model_v2(&ManagedModelV2Upsert {
                previous_slug: Some(model_slug.to_string()),
                model,
            })
            .expect("save model fast policy");

        let mut request = serde_json::json!({
            "model": model_slug,
            "input": "hello",
            "stream": false
        });
        if let Some(client_tier) = client_tier {
            request["service_tier"] = serde_json::Value::String(client_tier.to_string());
        }
        let server = codexmanager_service::start_one_shot_server().expect("start server");
        let (status, response_body) = post_http_raw(
            &server.addr,
            "/v1/responses",
            &request.to_string(),
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
        let upstream_body: serde_json::Value =
            serde_json::from_slice(&decode_upstream_request_body(&captured))
                .expect("parse upstream body");
        assert_eq!(
            upstream_body
                .get("service_tier")
                .and_then(serde_json::Value::as_str),
            expected_upstream_tier,
            "unexpected upstream service_tier for {policy:?}"
        );
    }

    let mut model = storage
        .get_managed_model_v2(model_slug)
        .expect("get model")
        .expect("model exists");
    model.fast_policy = ModelFastPolicyV2::Block;
    storage
        .upsert_managed_model_v2(&ManagedModelV2Upsert {
            previous_slug: Some(model_slug.to_string()),
            model,
        })
        .expect("save block policy");
    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &serde_json::json!({
            "model": model_slug,
            "input": "hello",
            "stream": false,
            "service_tier": "fast"
        })
        .to_string(),
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 400, "gateway response: {response_body}");
    assert!(
        response_body.contains("does not allow Fast requests"),
        "unexpected block response: {response_body}"
    );
    assert!(
        upstream_rx
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "blocked request must not reach upstream"
    );
    upstream_join.join().expect("join upstream");
}
