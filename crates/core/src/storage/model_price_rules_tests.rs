use super::*;

fn collect_query_plan_details_with_params(
    storage: &Storage,
    sql: &str,
    params: Vec<Value>,
) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query(params_from_iter(params)).expect("query explain");
    collect_query_plan_rows(&mut rows)
}

fn collect_query_plan_rows(rows: &mut rusqlite::Rows<'_>) -> Vec<String> {
    let mut details = Vec::new();
    while let Some(row) = rows.next().expect("next explain row") {
        let detail: String = row.get(3).expect("detail");
        details.push(detail.to_ascii_lowercase());
    }
    details
}

fn price_rule(id: &str, model_pattern: &str, source: &str, priority: i64) -> ModelPriceRule {
    ModelPriceRule {
        id: id.to_string(),
        provider: "openai".to_string(),
        model_pattern: model_pattern.to_string(),
        match_type: "exact".to_string(),
        billing_mode: "standard".to_string(),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        input_price_per_1m: Some(1.0),
        cached_input_price_per_1m: None,
        output_price_per_1m: Some(2.0),
        reasoning_output_price_per_1m: None,
        cache_write_5m_price_per_1m: None,
        cache_write_1h_price_per_1m: None,
        cache_hit_price_per_1m: None,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source: source.to_string(),
        source_url: None,
        seed_version: None,
        enabled: true,
        priority,
        created_at: 1,
        updated_at: 1,
    }
}

#[test]
fn count_model_price_rules_for_seed_uses_source_seed_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_price_rule_count_for_seed_sql()
        ),
        vec![Value::Text("2026-06".to_string())],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_source_seed")),
        "expected seed count to use source/seed index, got {details:?}"
    );
}
#[test]
fn find_enabled_custom_exact_model_price_rule_uses_lookup_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_custom_exact_model_price_rule_sql()
        ),
        vec![Value::Text("gpt-5".to_string())],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_custom_exact_lookup")),
        "expected custom exact lookup to use index, got {details:?}"
    );
}

#[test]
fn find_enabled_custom_exact_model_price_rule_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let official = price_rule("official", "gpt-5", "official_seed", 30_000);
    let low = price_rule("custom-low", "gpt-5", "custom", 100);
    let high = price_rule("custom-high", "GPT-5", "custom", 200);
    storage
        .upsert_model_price_rule(&official)
        .expect("insert official");
    storage
        .upsert_model_price_rule(&low)
        .expect("insert low custom");
    storage
        .upsert_model_price_rule(&high)
        .expect("insert high custom");

    let rule = storage
        .find_enabled_custom_exact_model_price_rule(" gpt-5 ")
        .expect("find rule")
        .expect("rule exists");

    assert_eq!(rule.id, "custom-high");
}

#[test]
fn enabled_model_price_rule_pattern_lookup_uses_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = enabled_model_price_rule_patterns_for_patterns_chunk_sql(
        "LOWER(TRIM(model_pattern)) IN ('gpt-5', 'claude-test')",
    );
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {sql}"),
        vec![Value::Integer(1)],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_enabled_pattern_lookup")),
        "expected enabled pattern lookup to use index, got {details:?}"
    );
    assert!(
        !details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "enabled pattern lookup chunk should avoid per-chunk ORDER BY temp sorting, got {details:?}"
    );
}

#[test]
fn list_enabled_model_price_rule_patterns_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut enabled = price_rule("enabled", " GPT-5 ", "official_seed", 1);
    let mut disabled = price_rule("disabled", "claude-disabled", "custom", 1);
    disabled.enabled = false;
    storage
        .upsert_model_price_rule(&enabled)
        .expect("insert enabled");
    storage
        .upsert_model_price_rule(&disabled)
        .expect("insert disabled");

    let patterns = storage
        .list_enabled_model_price_rule_patterns_for_patterns(&[
            "gpt-5".to_string(),
            "CLAUDE-DISABLED".to_string(),
            "missing".to_string(),
        ])
        .expect("list patterns");

    assert_eq!(patterns, vec!["gpt-5".to_string()]);

    enabled.enabled = false;
    storage
        .upsert_model_price_rule(&enabled)
        .expect("disable enabled");
    let patterns = storage
        .list_enabled_model_price_rule_patterns_for_patterns(&["gpt-5".to_string()])
        .expect("list disabled patterns");
    assert!(patterns.is_empty());
}
