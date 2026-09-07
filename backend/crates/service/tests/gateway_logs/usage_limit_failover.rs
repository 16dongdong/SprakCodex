use super::*;
use codexmanager_core::storage::UsageSnapshotRecord;

fn seed_two_healthy_accounts_and_key(
    storage: &Storage,
    primary_id: &str,
    secondary_id: &str,
    platform_key: &str,
    key_id: &str,
) {
    let now = now_ts();
    for (id, sort) in [(primary_id, 0_i64), (secondary_id, 1_i64)] {
        storage
            .insert_account(&Account {
                id: id.to_string(),
                label: id.to_string(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some(format!("chatgpt_{id}")),
                workspace_id: None,
                group_name: None,
                sort,
                status: "active".to_string(),
                created_at: now + sort,
                updated_at: now + sort,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: id.to_string(),
                id_token: String::new(),
                access_token: format!("access_{id}"),
                refresh_token: String::new(),
                api_key_access_token: Some(format!("api_access_{id}")),
                last_refresh: now,
            })
            .expect("insert token");
        storage
            .insert_usage_snapshot(&UsageSnapshotRecord {
                account_id: id.to_string(),
                used_percent: Some(10.0),
                window_minutes: Some(300),
                resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                credits_json: None,
                captured_at: now,
            })
            .expect("insert snapshot");
    }

    storage
        .insert_api_key(&ApiKey {
            id: key_id.to_string(),
            name: Some(key_id.to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
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
}

fn assert_primary_then_secondary(
    upstream_rx: &Receiver<CapturedUpstreamRequest>,
    primary_id: &str,
    secondary_id: &str,
) {
    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive primary upstream request");
    let second = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive secondary upstream request");
    let first_auth = first
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        first_auth.contains(format!("access_{primary_id}").as_str()),
        "首次应命中 primary 账号，实际 auth 头：{first_auth}"
    );
    let second_auth = second
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        second_auth.contains(format!("access_{secondary_id}").as_str()),
        "失败后应在同一请求续切 secondary，实际 auth 头：{second_auth}"
    );
}

/// 当首个账号用 200 + SSE `data:` 正文夹带 usage-limit 回应时，网关必须在尚未
/// 向客户端提交响应前识别它，并在同一个客户端请求内切到下一个账号。
#[test]
fn gateway_usage_limit_in_initial_sse_fails_over_before_delivery() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-usage-limit-sse-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let usage_limit_sse = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_limited_primary\"}}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"message\",\"content\":[]}}\n\n",
        "data: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"You've hit your usage limit. To get more access now, send a request to your admin or try again at 7:44 PM.\"}\n\n",
        "data: [DONE]\n\n"
    );
    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"secondary ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_secondary_ok\",\"model\":\"gpt-5.3-codex\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
        "data: [DONE]\n\n"
    );

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    usage_limit_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
                (200, ok_sse.to_string(), "text/event-stream".to_string()),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();

    // 两个候选账号都健康（10%），以保证 has_more_candidates=true 让
    // should_failover_for_gateway_error 返回 true，走 failover 标记分支。
    for (id, sort) in [("acc_primary", 0_i64), ("acc_secondary", 1_i64)] {
        storage
            .insert_account(&Account {
                id: id.to_string(),
                label: id.to_string(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some(format!("chatgpt_{id}")),
                workspace_id: None,
                group_name: None,
                sort,
                status: "active".to_string(),
                created_at: now + sort,
                updated_at: now + sort,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: id.to_string(),
                id_token: String::new(),
                access_token: format!("access_{id}"),
                refresh_token: String::new(),
                api_key_access_token: Some(format!("api_access_{id}")),
                last_refresh: now,
            })
            .expect("insert token");
        storage
            .insert_usage_snapshot(&UsageSnapshotRecord {
                account_id: id.to_string(),
                used_percent: Some(10.0),
                window_minutes: Some(300),
                resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                credits_json: None,
                captured_at: now,
            })
            .expect("insert snapshot");
    }

    let platform_key = "pk_usage_limit_failover_marker";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_usage_limit_failover_marker".to_string(),
            name: Some("usage-limit-failover-marker".to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
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
    let req_body_json = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    });
    let req_body = serde_json::to_string(&req_body_json).expect("serialize request");
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");
    assert!(
        response_body.contains("secondary ok"),
        "客户端应只收到第二个账号的成功流：{response_body}"
    );
    assert!(
        !response_body.contains("usage limit"),
        "首账号额度错误不得泄漏给客户端：{response_body}"
    );
    assert_eq!(
        storage
            .find_account_status_by_id("acc_primary")
            .expect("read primary status")
            .as_deref(),
        Some("active"),
        "仅凭 output_text 额度提示不能永久停用账号"
    );

    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive primary upstream request");
    let second = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive secondary upstream request");
    upstream_join.join().expect("join mock upstream");
    let first_auth = first
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        first_auth.contains("access_acc_primary"),
        "首次应命中 sort=0 的 primary 账号，实际 auth 头：{first_auth}"
    );
    let second_auth = second
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        second_auth.contains("access_acc_secondary"),
        "额度耗尽后应在同一请求续切 secondary，实际 auth 头：{second_auth}"
    );

    // 等 request log 异步落盘。
    let mut log = None;
    for _ in 0..40 {
        let logs = storage
            .list_request_logs(Some("key:=gk_usage_limit_failover_marker"), 20)
            .expect("list request logs");
        log = logs
            .into_iter()
            .find(|item| item.request_path == "/v1/responses");
        if log.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let log = log.expect("request log should be recorded");
    assert_eq!(
        log.status_code,
        Some(200),
        "最终成功请求应按第二个账号的 200 记账，实际 {:?}",
        log.status_code
    );
    assert_eq!(
        log.account_id.as_deref(),
        Some("acc_secondary"),
        "最终请求日志应记录真正完成响应的 secondary 账号"
    );
}

/// 一旦首账号已经产出普通语义内容，请求即已提交给客户端；随后到达的额度错误可以
/// 更新账号状态，但不得在同一请求内续切账号，以免重复输出或重复执行工具副作用。
#[test]
fn gateway_usage_limit_after_semantic_delta_does_not_fail_over_same_request() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-post-semantic-usage-limit");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let failed_after_semantic_sse = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_semantic_primary\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"primary visible answer\"}\n\n",
        "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_semantic_primary\",\"status\":\"failed\",\"error\":{\"code\":\"usage_limit_reached\",\"message\":\"The usage limit has been reached\"}}}\n\n",
        "data: [DONE]\n\n"
    );
    let unused_secondary_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"secondary must not run\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_unused_secondary\",\"status\":\"completed\"}}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    failed_after_semantic_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
                (
                    200,
                    unused_secondary_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
            ],
            Duration::from_millis(300),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let platform_key = "pk_post_semantic_usage_limit";
    seed_two_healthy_accounts_and_key(
        &storage,
        "acc_semantic_primary",
        "acc_semantic_secondary",
        platform_key,
        "gk_post_semantic_usage_limit",
    );

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    })
    .to_string();
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("primary visible answer"),
        "已提交的首账号语义输出应保持可见：{gateway_body}"
    );
    assert!(
        !gateway_body.contains("secondary must not run"),
        "语义输出提交后不得将第二账号拼入同一响应：{gateway_body}"
    );
    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive primary upstream request");
    upstream_join.join().expect("join mock upstream");
    assert!(
        first
            .headers
            .get("authorization")
            .is_some_and(|value| value.contains("access_acc_semantic_primary")),
        "首次请求应命中 primary 账号：{:?}",
        first.headers.get("authorization")
    );
    assert!(
        upstream_rx.try_recv().is_err(),
        "首账号已产出语义 delta 后，response.failed 不得触发第二次上游请求"
    );
}

/// 首个可操作事件就是明确的 usage-limit `response.failed` 时，客户端尚未收到语义
/// 输出；网关应在同一请求内切到第二账号，并把首账号持久标记为 limited。
#[test]
fn gateway_explicit_usage_limit_failure_fails_over_and_marks_account_limited() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-explicit-usage-limit-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let explicit_usage_limit_sse = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_explicit_limited\"}}\n\n",
        "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_explicit_limited\",\"status\":\"failed\",\"error\":{\"code\":\"usage_limit_reached\",\"message\":\"The usage limit has been reached\"}}}\n\n"
    );
    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"secondary explicit failover ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_explicit_secondary\",\"status\":\"completed\",\"model\":\"gpt-5.3-codex\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    explicit_usage_limit_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
                (200, ok_sse.to_string(), "text/event-stream".to_string()),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let platform_key = "pk_explicit_usage_limit_failover";
    seed_two_healthy_accounts_and_key(
        &storage,
        "acc_explicit_primary",
        "acc_explicit_secondary",
        platform_key,
        "gk_explicit_usage_limit_failover",
    );

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    })
    .to_string();
    // Leave enough time for the 10-second stream preflight cap so a classifier
    // regression fails on response content instead of a test socket timeout.
    let (status, gateway_body) = post_http_raw_with_read_timeout(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
        Duration::from_secs(15),
    );
    server.join();

    upstream_join.join().expect("join mock upstream");
    let upstream_auths = upstream_rx
        .try_iter()
        .map(|request| {
            request
                .headers
                .get("authorization")
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        status, 200,
        "gateway response: {gateway_body}; upstream auth sequence: {upstream_auths:?}"
    );
    assert!(
        gateway_body.contains("secondary explicit failover ok"),
        "首账号明确额度失败后应交付第二账号结果：{gateway_body}"
    );
    assert!(
        !gateway_body.contains("resp_explicit_limited"),
        "首账号的失败事件不得泄漏给客户端：{gateway_body}"
    );
    assert_eq!(
        upstream_auths.len(),
        2,
        "explicit usage-limit failover must make exactly two upstream requests: {upstream_auths:?}"
    );
    assert!(upstream_auths[0].contains("access_acc_explicit_primary"));
    assert!(upstream_auths[1].contains("access_acc_explicit_secondary"));

    let primary = storage
        .find_account_by_id("acc_explicit_primary")
        .expect("find explicit-limited primary account")
        .expect("explicit-limited primary account exists");
    assert_eq!(primary.status, "limited");
    let reasons = storage
        .latest_account_status_reasons(&["acc_explicit_primary".to_string()])
        .expect("load explicit-limited account status reason");
    assert_eq!(
        reasons.get("acc_explicit_primary").map(String::as_str),
        Some("usage_limit_exhausted")
    );
}

/// `/v1/responses` 的非流式客户端响应仍固定从上游读取 SSE；额度错误预检必须依据
/// 实际上游传输模式执行，不能因客户端传入 `stream:false` 而跳过。
#[test]
fn gateway_non_stream_responses_usage_limit_sse_fails_over_before_delivery() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-non-stream-usage-limit-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let usage_limit_sse = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_non_stream_limited\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"You've hit your usage limit. Please try again later.\"}\n\n",
        "data: [DONE]\n\n"
    );
    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"secondary non-stream ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_non_stream_secondary\",\"status\":\"completed\",\"model\":\"gpt-5.3-codex\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"secondary non-stream ok\"}]}],\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    usage_limit_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
                (200, ok_sse.to_string(), "text/event-stream".to_string()),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let platform_key = "pk_non_stream_usage_limit_failover";
    seed_two_healthy_accounts_and_key(
        &storage,
        "acc_non_stream_primary",
        "acc_non_stream_secondary",
        platform_key,
        "gk_non_stream_usage_limit_failover",
    );

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": false
    })
    .to_string();
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("secondary non-stream ok"),
        "非流式客户端应收到第二账号聚合后的成功响应：{gateway_body}"
    );
    assert!(
        !gateway_body.contains("usage limit"),
        "首账号额度错误不得泄漏给非流式客户端：{gateway_body}"
    );
    let response_json: serde_json::Value =
        serde_json::from_str(&gateway_body).expect("非流式响应必须是单个合法 JSON 文档");
    assert_eq!(
        response_json
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
    assert_eq!(
        response_json
            .pointer("/output/0/content/0/text")
            .and_then(serde_json::Value::as_str),
        Some("secondary non-stream ok")
    );
    assert_primary_then_secondary(
        &upstream_rx,
        "acc_non_stream_primary",
        "acc_non_stream_secondary",
    );
    upstream_join.join().expect("join mock upstream");
}

/// 若首账号只发出结构性 metadata 事件就正常结束连接，客户端尚未收到任何有效输出；
/// 网关应把该不完整流视作可重试的传输失败，并在同一请求内切到下一账号。
#[test]
fn gateway_metadata_only_sse_eof_fails_over_before_delivery() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-metadata-eof-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let metadata_only_sse =
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_abandoned_primary\"}}\n\n";
    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"secondary after eof ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_eof_secondary\",\"model\":\"gpt-5.3-codex\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    metadata_only_sse.to_string(),
                    "text/event-stream".to_string(),
                ),
                (200, ok_sse.to_string(), "text/event-stream".to_string()),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let platform_key = "pk_metadata_eof_failover";
    seed_two_healthy_accounts_and_key(
        &storage,
        "acc_eof_primary",
        "acc_eof_secondary",
        platform_key,
        "gk_metadata_eof_failover",
    );

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    })
    .to_string();
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("secondary after eof ok"),
        "metadata-only EOF 后应收到第二账号的成功流：{gateway_body}"
    );
    assert!(
        !gateway_body.contains("resp_abandoned_primary"),
        "首账号 metadata 不得泄漏给客户端：{gateway_body}"
    );
    assert_primary_then_secondary(&upstream_rx, "acc_eof_primary", "acc_eof_secondary");
    upstream_join.join().expect("join mock upstream");
}

/// Fix B 端到端：快要耗尽的账号（99% used）即使 sort 排前，也应被降权到候选尾部，
/// 首个请求直接命中健康账号，不必经历失败-重试流程。
#[test]
fn gateway_low_quota_account_is_skipped_on_first_request() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-low-quota-skip");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_lowq_ok\",\"model\":\"gpt-5.3-codex\",\"usage\":{\"input_tokens\":3,\"output_tokens\":1,\"total_tokens\":4}}}\n\n",
        "data: [DONE]\n\n"
    );

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(200, ok_sse.to_string(), "text/event-stream".to_string())],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();

    // sort=0 的账号快照 99%（快耗尽），sort=1 的健康（10%）。
    // Fix B 应把 exhausted 排到尾部，实际请求只打 healthy。
    let rows: Vec<(&str, i64, f64)> = vec![("acc_exhausted", 0, 99.0), ("acc_healthy", 1, 10.0)];
    for (id, sort, used_pct) in &rows {
        storage
            .insert_account(&Account {
                id: (*id).to_string(),
                label: (*id).to_string(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some(format!("chatgpt_{id}")),
                workspace_id: None,
                group_name: None,
                sort: *sort,
                status: "active".to_string(),
                created_at: now + *sort,
                updated_at: now + *sort,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: (*id).to_string(),
                id_token: String::new(),
                access_token: format!("access_{id}"),
                refresh_token: String::new(),
                api_key_access_token: Some(format!("api_access_{id}")),
                last_refresh: now,
            })
            .expect("insert token");
        storage
            .insert_usage_snapshot(&UsageSnapshotRecord {
                account_id: (*id).to_string(),
                used_percent: Some(*used_pct),
                window_minutes: Some(300),
                resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                credits_json: None,
                captured_at: now,
            })
            .expect("insert snapshot");
    }

    let platform_key = "pk_low_quota_skip";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_low_quota_skip".to_string(),
            name: Some("low-quota-skip".to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
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
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    });
    let request_body = serde_json::to_string(&request_body).expect("serialize request");
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {gateway_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");

    let auth = captured
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        auth.contains("access_acc_healthy"),
        "即便 sort=0 的账号排在前，99% used 的账号也应该被降到尾部；实际 auth 头：{auth}"
    );
    assert!(
        upstream_rx
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "低配额账号应被直接跳过，不应再有第二次上游请求"
    );
}
