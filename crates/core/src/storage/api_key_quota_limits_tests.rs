use super::*;

fn collect_query_plan_details(storage: &Storage, sql: &str) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut details = Vec::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        details.push(detail.to_ascii_lowercase());
    }
    details
}

#[test]
fn api_key_total_usage_filters_each_stats_table_by_key_id() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        api_key_total_token_usage_sql().replace("?1", "'key-quota-1'")
    );

    let details = collect_query_plan_details(&storage, &sql);

    assert!(
        details.iter().any(|detail| {
            detail.contains("search request_token_stats")
                && detail.contains("using index")
                && detail.contains("key_id=?")
        }),
        "expected raw stats key lookup index search in plan, got {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_request_token_stat_hourly_rollups_key_bucket")),
        "expected hourly rollup key lookup index in plan, got {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_request_token_stat_rollups_key_id")),
        "expected legacy rollup key lookup index in plan, got {details:?}"
    );
}

#[test]
fn api_key_remaining_quota_usage_scopes_stats_to_limited_keys() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        api_key_remaining_quota_tokens_sql()
    );

    let details = collect_query_plan_details(&storage, &sql);

    assert!(
        details.iter().any(|detail| {
            detail.contains("search s")
                && detail.contains("idx_request_token_stats_success_key_model_created_at")
                && detail.contains("key_id=?")
        }),
        "expected limited-key raw usage lookup by key index, got {details:?}"
    );
    assert!(
        details.iter().any(|detail| {
            detail.contains("search h")
                && detail.contains("idx_request_token_stat_hourly_rollups_key_bucket")
        }),
        "expected limited-key hourly usage lookup by key index, got {details:?}"
    );
    assert!(
        details.iter().any(|detail| {
            detail.contains("search r") && detail.contains("idx_request_token_stat_rollups_key_id")
        }),
        "expected limited-key legacy rollup lookup by key index, got {details:?}"
    );
}

#[test]
fn api_key_quota_overview_stats_reads_key_and_usage_tables_directly() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = format!("EXPLAIN QUERY PLAN {}", api_key_quota_overview_stats_sql());

    let details = collect_query_plan_details(&storage, &sql);

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("scan k") || detail.contains("scan api_keys")),
        "expected quota overview to scan api keys directly, got {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_api_key_quota_limits_1")),
        "expected quota overview to join quota limits by primary key, got {details:?}"
    );
    for aliases in [
        ["scan s", "search s"],
        ["scan h", "search h"],
        ["scan r", "search r"],
    ] {
        assert!(
            details
                .iter()
                .any(|detail| aliases.iter().any(|alias| detail.contains(alias))),
            "expected quota overview to aggregate token usage source {aliases:?}, got {details:?}"
        );
    }
}

#[test]
fn api_key_quota_limits_for_ids_uses_key_lookup_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = api_key_quota_limits_for_ids_chunk_sql("key_id IN ('key-a', 'key-b')");
    let details = collect_query_plan_details(&storage, &format!("EXPLAIN QUERY PLAN {sql}"));

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_api_key_quota_limits_1")),
        "expected quota limit chunk query to use quota limit primary-key lookup, got {details:?}"
    );
    assert!(
        !details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "quota limit chunk query should avoid per-chunk ORDER BY temp sorting, got {details:?}"
    );
}

#[test]
fn api_key_quota_limit_lookup_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        api_key_quota_limit_value_by_key_sql().replace("?1", "'key-quota-1'")
    );

    let details = collect_query_plan_details(&storage, &sql);

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_api_key_quota_limits_1")),
        "expected quota limit lookup to use quota limit primary-key index, got {details:?}"
    );

    let delete_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        delete_api_key_quota_limit_by_key_sql().replace("?1", "'key-quota-1'")
    );
    let delete_details = collect_query_plan_details(&storage, &delete_sql);
    assert!(
        delete_details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_api_key_quota_limits_1")),
        "expected quota limit delete to use quota limit primary-key index, got {delete_details:?}"
    );
}

#[test]
fn api_key_total_usage_ignores_blank_key_ids() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .conn
        .execute(
            "INSERT INTO request_token_stats (
                request_log_id, key_id, account_id, model,
                input_tokens, cached_input_tokens, output_tokens, total_tokens,
                reasoning_output_tokens, estimated_cost_usd, created_at
             ) VALUES (1, '', NULL, 'gpt-test', 1, 0, 1, 2, 0, 0.0, 1)",
            [],
        )
        .expect("seed blank key usage");

    assert_eq!(
        storage
            .api_key_total_token_usage(" ")
            .expect("blank key usage"),
        0
    );
}
