use super::*;

fn collect_query_plan_details(storage: &Storage, sql: &str) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    collect_query_plan_rows(&mut rows)
}

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

#[test]
fn model_source_lookup_queries_use_composite_indexes() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");

    let source_model_list_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_models_list_sql(true, true)
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(source_model_list_details
        .iter()
        .any(|detail| { detail.contains("sqlite_autoindex_model_source_models_1") }));
    assert!(
        !source_model_list_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "model source list query should preserve primary-key order without temp sorting, got {source_model_list_details:?}"
    );

    let source_model_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            available_source_model_ids_by_upstream_model_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("gpt-upstream".to_string()),
        ],
    );
    assert!(source_model_details
        .iter()
        .any(|detail| { detail.contains("idx_model_source_models_kind_upstream_status_source") }));

    let source_model_exists_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", available_source_model_exists_sql()),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
            Value::Text("gpt-upstream".to_string()),
        ],
    );
    assert!(source_model_exists_details.iter().any(|detail| {
        detail.contains("search model_source_models") && detail.contains("index")
    }));

    let available_source_model_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            available_model_source_models_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(available_source_model_details
        .iter()
        .any(|detail| { detail.contains("idx_model_source_models_source_status_upstream") }));

    let mapping_list_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_mappings_list_sql(true)
        ),
        vec![Value::Text("gpt-platform".to_string())],
    );
    assert!(mapping_list_details.iter().any(|detail| {
        detail.contains("idx_model_source_mappings_platform_enabled_priority_weight")
    }));

    let platform_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mappings_for_platform_sql()
        ),
        vec![Value::Text("gpt-platform".to_string())],
    );
    assert!(platform_details.iter().any(|detail| {
        detail.contains("idx_model_source_mappings_platform_enabled_priority_weight")
    }));

    let platform_kind_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mappings_for_platform_and_kind_sql()
        ),
        vec![
            Value::Text("gpt-platform".to_string()),
            Value::Text("openai_account".to_string()),
        ],
    );
    assert!(
        platform_kind_details.iter().any(|detail| {
            detail.contains("idx_model_source_mappings_platform_kind_enabled_priority")
        }),
        "expected platform/kind mapping list to use platform/kind order index, got {platform_kind_details:?}"
    );
    assert!(
        !platform_kind_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "expected platform/kind mapping list to avoid temp sorting, got {platform_kind_details:?}"
    );

    let platform_exists_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mapping_exists_for_platform_sql()
        ),
        vec![Value::Text("gpt-platform".to_string())],
    );
    assert!(platform_exists_details.iter().any(|detail| {
        detail.contains("idx_model_source_mappings_platform_enabled_priority_weight")
    }));

    let platform_batch_details = collect_query_plan_details(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mapping_platform_slugs_for_platforms_chunk_sql(
                "platform_model_slug IN ('gpt-platform', 'missing-platform')"
            )
        ),
    );
    assert!(platform_batch_details.iter().any(|detail| {
        detail.contains("search model_source_mappings") && detail.contains("index")
    }));
    assert!(
        !platform_batch_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "platform slug chunk query should avoid per-chunk ORDER BY temp sorting, got {platform_batch_details:?}"
    );

    let source_model_batch_details = collect_query_plan_details(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_model_upstream_models_for_upstream_models_chunk_sql(
                "upstream_model IN ('gpt-upstream', 'missing-upstream')"
            )
        ),
    );
    assert!(source_model_batch_details
        .iter()
        .any(|detail| detail.contains("idx_model_source_models_upstream_model")));
    assert!(
        !source_model_batch_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "upstream model chunk query should avoid per-chunk ORDER BY temp sorting, got {source_model_batch_details:?}"
    );

    let source_id_by_kind_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_model_source_ids_for_kind_sql()
        ),
        vec![Value::Text("aggregate_api".to_string())],
    );
    assert!(source_id_by_kind_details
        .iter()
        .any(|detail| { detail.contains("idx_model_source_models_source_status_upstream") }));

    let platform_source_id_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mapping_source_ids_for_platform_and_kind_sql()
        ),
        vec![
            Value::Text("gpt-platform".to_string()),
            Value::Text("openai_account".to_string()),
        ],
    );
    assert!(platform_source_id_details.iter().any(|detail| {
        detail.contains("idx_model_source_mappings_platform_source_enabled_priority")
    }));
    let mapping_source_id_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_mapping_source_ids_for_kind_sql()
        ),
        vec![Value::Text("openai_account".to_string())],
    );
    assert!(mapping_source_id_details
        .iter()
        .any(|detail| detail.contains("idx_model_source_mappings_source")));
    assert!(
        !mapping_source_id_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "expected mapping source id list to avoid temp sorting, got {mapping_source_id_details:?}"
    );

    let source_platform_slug_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_mapping_platform_slugs_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(source_platform_slug_details
        .iter()
        .any(|detail| detail.contains("idx_model_source_mappings_source_platform")));
    assert!(
        !source_platform_slug_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "expected source platform slug list to avoid temp sorting, got {source_platform_slug_details:?}"
    );

    let kind_platform_slug_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mapping_platform_slugs_for_kind_sql()
        ),
        vec![Value::Text("openai_account".to_string())],
    );
    assert!(kind_platform_slug_details
        .iter()
        .any(|detail| detail.contains("idx_model_source_mappings_kind_enabled_platform")));
    assert!(
        !kind_platform_slug_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "expected kind platform slug list to avoid temp sorting, got {kind_platform_slug_details:?}"
    );

    let source_mapping_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_model_source_mapping_for_platform_source_sql()
        ),
        vec![
            Value::Text("gpt-platform".to_string()),
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(source_mapping_details.iter().any(|detail| {
        detail.contains("idx_model_source_mappings_platform_source_enabled_priority")
    }));

    let source_mapping_chunk_sql = enabled_model_source_mappings_for_sources_chunk_sql(
        "source_id IN ('acc-routing-1', 'acc-routing-2')",
    );
    let source_mapping_chunk_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {source_mapping_chunk_sql}"),
        vec![
            Value::Text("gpt-platform".to_string()),
            Value::Text("openai_account".to_string()),
        ],
    );
    assert!(
        source_mapping_chunk_details.iter().any(|detail| {
            detail.contains("idx_model_source_mappings_platform_source_enabled_priority")
        }),
        "expected source mapping chunk query to use platform/source lookup index, got {source_mapping_chunk_details:?}"
    );

    let mapping_delete_by_id_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mapping_by_id_sql()
        ),
        vec![Value::Text("mapping-1".to_string())],
    );
    assert!(mapping_delete_by_id_details
        .iter()
        .any(|detail| detail.contains("sqlite_autoindex_model_source_mappings_1")));

    let mapping_delete_for_source_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mappings_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(mapping_delete_for_source_details
        .iter()
        .any(|detail| detail.contains("idx_model_source_mappings_source")));

    let mapping_delete_for_source_upstream_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mappings_for_source_upstream_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
            Value::Text("gpt-upstream".to_string()),
        ],
    );
    assert!(mapping_delete_for_source_upstream_details
        .iter()
        .any(|detail| {
            detail.contains("idx_model_source_mappings_source")
                || detail.contains("sqlite_autoindex_model_source_mappings_2")
        }));

    let source_model_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_models_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(source_model_delete_details.iter().any(|detail| {
        detail.contains("idx_model_source_models_source_status_upstream")
            || detail.contains("sqlite_autoindex_model_source_models_1")
    }));

    let source_model_discovery_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_model_for_source_discovery_upstream_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
            Value::Text("gpt-upstream".to_string()),
            Value::Text("remote".to_string()),
        ],
    );
    assert!(source_model_discovery_delete_details.iter().any(|detail| {
        detail.contains("idx_model_source_models_source_status_upstream")
            || detail.contains("sqlite_autoindex_model_source_models_1")
    }));

    let platform_model_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mappings_for_platform_model_sql()
        ),
        vec![Value::Text("gpt-platform".to_string())],
    );
    assert!(platform_model_delete_details.iter().any(|detail| {
        detail.contains("search model_source_mappings") && detail.contains("index")
    }));

    let preference_delete_one_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mapping_preference_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
            Value::Text("gpt-upstream".to_string()),
        ],
    );
    assert!(preference_delete_one_details
        .iter()
        .any(|detail| { detail.contains("sqlite_autoindex_model_source_mapping_preferences_1") }));

    let preference_list_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_source_mapping_preferences_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(preference_list_details.iter().any(|detail| {
        detail.contains("idx_model_source_mapping_preferences_source")
            || detail.contains("sqlite_autoindex_model_source_mapping_preferences_1")
    }));

    let preference_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_source_mapping_preferences_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-routing-1".to_string()),
        ],
    );
    assert!(preference_delete_details.iter().any(|detail| {
        detail.contains("idx_model_source_mapping_preferences_source")
            || detail.contains("sqlite_autoindex_model_source_mapping_preferences_1")
    }));
}

#[test]
fn list_model_source_model_source_ids_for_kind_returns_distinct_non_empty_ids() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");
    let now = now_ts();
    for (source_kind, source_id, upstream_model) in [
        ("aggregate_api", "agg-b", "gpt-b"),
        ("aggregate_api", "agg-a", "gpt-a"),
        ("aggregate_api", "agg-a", "gpt-a-2"),
        ("aggregate_api", "", "gpt-empty"),
        ("openai_account", "acc-a", "gpt-a"),
    ] {
        storage
            .upsert_model_source_model(&ModelSourceModel {
                source_kind: source_kind.to_string(),
                source_id: source_id.to_string(),
                upstream_model: upstream_model.to_string(),
                display_name: None,
                status: "available".to_string(),
                discovery_kind: "synced".to_string(),
                last_synced_at: Some(now),
                extra_json: "{}".to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("seed source model");
    }

    let source_ids = storage
        .list_model_source_model_source_ids_for_kind(" aggregate_api ")
        .expect("list source ids");

    assert_eq!(source_ids, vec!["agg-a".to_string(), "agg-b".to_string()]);
}

#[test]
fn list_model_route_source_ids_for_kind_unions_models_and_mappings() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");
    let now = now_ts();
    for (source_kind, source_id, upstream_model) in [
        ("aggregate_api", "agg-model-only", "gpt-model"),
        ("aggregate_api", "agg-shared", "gpt-shared"),
        ("aggregate_api", "", "gpt-empty"),
        ("openai_account", "acc-ignored", "gpt-ignored"),
    ] {
        storage
            .upsert_model_source_model(&ModelSourceModel {
                source_kind: source_kind.to_string(),
                source_id: source_id.to_string(),
                upstream_model: upstream_model.to_string(),
                display_name: None,
                status: "available".to_string(),
                discovery_kind: "synced".to_string(),
                last_synced_at: None,
                extra_json: "{}".to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("seed source model");
    }
    for (id, source_kind, source_id) in [
        ("map-shared", "aggregate_api", "agg-shared"),
        ("map-only", "aggregate_api", "agg-mapping-only"),
        ("map-empty", "aggregate_api", ""),
        ("map-ignored", "openai_account", "acc-ignored"),
    ] {
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: id.to_string(),
                platform_model_slug: format!("platform-{id}"),
                source_kind: source_kind.to_string(),
                source_id: source_id.to_string(),
                upstream_model: format!("upstream-{id}"),
                enabled: true,
                priority: 0,
                weight: 1,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("seed mapping");
    }

    let source_ids = storage
        .list_model_route_source_ids_for_kind(" aggregate_api ")
        .expect("list route source ids");

    let plan = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_route_source_ids_for_kind_sql()
        ),
        vec![Value::Text("aggregate_api".to_string())],
    );
    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_model_source_models_source_status_upstream")),
        "expected route source model side to use source model covering index, got {plan:?}"
    );
    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_model_source_mappings_source")),
        "expected route mapping side to use mapping source index, got {plan:?}"
    );

    assert_eq!(
        source_ids,
        vec![
            "agg-mapping-only".to_string(),
            "agg-model-only".to_string(),
            "agg-shared".to_string()
        ]
    );
}

#[test]
fn platform_mapping_kind_existence_helpers_filter_source_kinds() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");
    let now = now_ts();
    for (id, source_kind, source_id, enabled) in [
        ("map-account", "openai_account", "acc-a", true),
        ("map-aggregate", "aggregate_api", "agg-a", true),
        ("map-disabled", "custom", "custom-a", false),
    ] {
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: id.to_string(),
                platform_model_slug: "gpt-platform".to_string(),
                source_kind: source_kind.to_string(),
                source_id: source_id.to_string(),
                upstream_model: format!("upstream-{id}"),
                enabled,
                priority: 0,
                weight: 1,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("seed mapping");
    }

    assert!(storage
        .has_enabled_model_source_mapping_for_platform_matching_kinds(
            "gpt-platform",
            &[" aggregate_api ", "missing_kind"],
        )
        .expect("matching kind exists"));
    assert!(!storage
        .has_enabled_model_source_mapping_for_platform_matching_kinds("gpt-platform", &["custom"],)
        .expect("disabled kind should not match"));
    assert!(storage
        .has_enabled_model_source_mapping_for_platform_outside_kinds(
            "gpt-platform",
            &["aggregate_api"],
        )
        .expect("outside kind exists"));
    assert!(!storage
        .has_enabled_model_source_mapping_for_platform_outside_kinds(
            "gpt-platform",
            &["aggregate_api", "openai_account"],
        )
        .expect("no outside kind exists"));
}

#[test]
fn list_enabled_model_source_mappings_for_sources_picks_top_mapping_per_source() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");
    let now = now_ts();
    for (id, source_id, upstream_model, priority, weight, enabled, source_kind) in [
        (
            "map-a-low",
            "acc-a",
            "gpt-low",
            0,
            10,
            true,
            "openai_account",
        ),
        (
            "map-a-top",
            "acc-a",
            "gpt-top",
            5,
            1,
            true,
            "openai_account",
        ),
        (
            "map-a-disabled",
            "acc-a",
            "gpt-disabled",
            9,
            1,
            false,
            "openai_account",
        ),
        (
            "map-b-weight-low",
            "acc-b",
            "gpt-z",
            2,
            1,
            true,
            "openai_account",
        ),
        (
            "map-b-weight-top",
            "acc-b",
            "gpt-a",
            2,
            2,
            true,
            "openai_account",
        ),
        (
            "map-kind-other",
            "acc-c",
            "gpt-other",
            9,
            9,
            true,
            "aggregate_api",
        ),
    ] {
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: id.to_string(),
                platform_model_slug: "gpt-platform".to_string(),
                source_kind: source_kind.to_string(),
                source_id: source_id.to_string(),
                upstream_model: upstream_model.to_string(),
                enabled,
                priority,
                weight,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("seed mapping");
    }

    let mappings = storage
        .list_enabled_model_source_mappings_for_sources(
            " gpt-platform ",
            " openai_account ",
            &[
                "acc-a".to_string(),
                "acc-b".to_string(),
                "acc-c".to_string(),
                "acc-a".to_string(),
                " ".to_string(),
            ],
        )
        .expect("list mappings");

    assert_eq!(mappings.len(), 2);
    assert_eq!(mappings["acc-a"].upstream_model, "gpt-top");
    assert_eq!(mappings["acc-b"].upstream_model, "gpt-a");
    assert!(!mappings.contains_key("acc-c"));
}

#[test]
fn list_enabled_model_source_mappings_for_sources_chunks_large_source_sets() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init schema");
    let now = now_ts();
    let target = "acc-batch-0742";
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "map-batch-target".to_string(),
            platform_model_slug: "gpt-platform".to_string(),
            source_kind: "openai_account".to_string(),
            source_id: target.to_string(),
            upstream_model: "gpt-batch-target".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed mapping");
    let source_ids = (0..950)
        .map(|index| format!("acc-batch-{index:04}"))
        .collect::<Vec<_>>();

    let mappings = storage
        .list_enabled_model_source_mappings_for_sources(
            "gpt-platform",
            "openai_account",
            &source_ids,
        )
        .expect("list mappings");

    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[target].upstream_model, "gpt-batch-target");
}
