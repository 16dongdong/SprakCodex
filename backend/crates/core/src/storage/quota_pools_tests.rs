use super::*;
use crate::storage::{now_ts, Account};

fn collect_query_plan_details(storage: &Storage, sql: &str) -> Vec<String> {
    collect_query_plan_details_with_params(storage, sql, Vec::new())
}

fn collect_query_plan_details_with_params(
    storage: &Storage,
    sql: &str,
    params: Vec<Value>,
) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query(params_from_iter(params)).expect("query explain");
    let mut details = Vec::new();
    while let Some(row) = rows.next().expect("next explain row") {
        let detail: String = row.get(3).expect("detail");
        details.push(detail.to_ascii_lowercase());
    }
    details
}

fn assert_no_temp_ordering(details: &[String], label: &str) {
    assert!(
        !details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "{label} should avoid temp ORDER BY sorting, got {details:?}"
    );
}

fn sample_account(id: &str, now: i64) -> Account {
    Account {
        id: id.to_string(),
        label: id.to_string(),
        issuer: "issuer".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn account_quota_helpers_filter_to_requested_accounts() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();

    for account_id in ["acc-a", "acc-b"] {
        storage
            .insert_account(&sample_account(account_id, now))
            .expect("insert account");
        storage
            .upsert_account_quota_capacity_override(account_id, Some(100), None)
            .expect("upsert override");
        storage
            .set_quota_source_model_assignments(
                "openai_account",
                account_id,
                &[format!("model-{account_id}")],
            )
            .expect("set assignments");
    }
    storage
        .set_quota_source_model_assignments(
            "aggregate_api",
            "acc-b",
            &["aggregate-model".to_string()],
        )
        .expect("set aggregate assignments");

    let requested = vec!["acc-b".to_string(), "missing".to_string()];
    let overrides = storage
        .list_account_quota_capacity_overrides_for_accounts(&requested)
        .expect("list overrides");
    let assignments = storage
        .list_quota_source_model_assignments_for_sources("openai_account", &requested)
        .expect("list assignments");

    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].account_id, "acc-b");
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].source_kind, "openai_account");
    assert_eq!(assignments[0].source_id, "acc-b");
    assert_eq!(assignments[0].model_slug, "model-acc-b");
}

#[test]
fn quota_source_model_assignments_for_kind_filters_source_kind() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .set_quota_source_model_assignments("aggregate_api", "agg-b", &["model-b".to_string()])
        .expect("set aggregate b assignments");
    storage
        .set_quota_source_model_assignments(
            "aggregate_api",
            "agg-a",
            &["model-c".to_string(), "model-a".to_string()],
        )
        .expect("set aggregate a assignments");
    storage
        .set_quota_source_model_assignments(
            "openai_account",
            "agg-a",
            &["account-model".to_string()],
        )
        .expect("set account assignments");

    let assignments = storage
        .list_quota_source_model_assignments_for_kind("aggregate_api")
        .expect("list aggregate assignments");

    assert_eq!(
        assignments
            .into_iter()
            .map(|item| (item.source_kind, item.source_id, item.model_slug))
            .collect::<Vec<_>>(),
        vec![
            (
                "aggregate_api".to_string(),
                "agg-a".to_string(),
                "model-a".to_string()
            ),
            (
                "aggregate_api".to_string(),
                "agg-a".to_string(),
                "model-c".to_string()
            ),
            (
                "aggregate_api".to_string(),
                "agg-b".to_string(),
                "model-b".to_string()
            )
        ]
    );
}

#[test]
fn quota_source_model_assignments_for_model_filters_with_model_index() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .set_quota_source_model_assignments(
            "openai_account",
            "acc-a",
            &["gpt-target".to_string(), "gpt-other".to_string()],
        )
        .expect("set account a assignments");
    storage
        .set_quota_source_model_assignments("openai_account", "acc-b", &["gpt-other".to_string()])
        .expect("set account b assignments");
    storage
        .set_quota_source_model_assignments("aggregate_api", "agg-a", &["gpt-target".to_string()])
        .expect("set aggregate assignments");

    let assignments = storage
        .list_quota_source_model_assignments_for_model("openai_account", "gpt-target")
        .expect("list model assignments");

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].source_kind, "openai_account");
    assert_eq!(assignments[0].source_id, "acc-a");
    assert_eq!(assignments[0].model_slug, "gpt-target");

    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_source_model_assignments_for_model_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("gpt-target".to_string()),
        ],
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_quota_source_model_assignments_model")),
        "quota assignment model lookup should use model index, got {details:?}"
    );
}

#[test]
fn quota_assignment_source_ids_for_kind_lists_distinct_sources() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .set_quota_source_model_assignments(
            "openai_account",
            "acc-b",
            &["gpt-b".to_string(), "gpt-a".to_string()],
        )
        .expect("set account b assignments");
    storage
        .set_quota_source_model_assignments("openai_account", "acc-a", &["gpt-a".to_string()])
        .expect("set account a assignments");
    storage
        .set_quota_source_model_assignments("aggregate_api", "agg-a", &["gpt-a".to_string()])
        .expect("set aggregate assignments");

    let source_ids = storage
        .list_quota_assignment_source_ids_for_kind("openai_account")
        .expect("list source ids");

    assert_eq!(source_ids, vec!["acc-a".to_string(), "acc-b".to_string()]);
}

#[test]
fn quota_source_model_assignment_targets_for_model_preserve_empty_implicit_sources() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .set_quota_source_model_assignments(
            "openai_account",
            "acc-target",
            &["gpt-target".to_string()],
        )
        .expect("set target assignment");
    storage
        .set_quota_source_model_assignments(
            "openai_account",
            "acc-other",
            &["gpt-other".to_string()],
        )
        .expect("set other assignment");
    storage
        .set_quota_source_model_assignments(
            "aggregate_api",
            "agg-target",
            &["gpt-target".to_string()],
        )
        .expect("set aggregate assignment");

    let targets = storage
        .list_quota_source_model_assignment_targets_for_model("openai_account", "gpt-target")
        .expect("list target assignments");

    assert_eq!(
        targets
            .iter()
            .map(|item| {
                (
                    item.source_kind.as_str(),
                    item.source_id.as_str(),
                    item.model_slug.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("openai_account", "acc-other", ""),
            ("openai_account", "acc-target", "gpt-target")
        ]
    );

    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_source_model_assignment_targets_for_model_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("gpt-target".to_string()),
        ],
    );
    assert!(
        details.iter().any(|detail| {
            detail.contains("idx_quota_source_model_assignments_source")
                || detail.contains("sqlite_autoindex_quota_source_model_assignments")
        }),
        "target assignment source lookup should use source index, got {details:?}"
    );
}

#[test]
fn account_quota_helpers_chunk_large_account_sets() {
    let mut storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();

    let target = "acc-0949";
    storage
        .insert_account(&sample_account(target, now))
        .expect("insert target account");
    storage
        .upsert_account_quota_capacity_override(target, Some(250), Some(500))
        .expect("upsert override");
    storage
        .set_quota_source_model_assignments("openai_account", target, &["gpt-large".to_string()])
        .expect("set assignments");

    let requested = (0..950)
        .map(|index| format!("acc-{index:04}"))
        .collect::<Vec<_>>();
    let overrides = storage
        .list_account_quota_capacity_overrides_for_accounts(&requested)
        .expect("list overrides");
    let assignments = storage
        .list_quota_source_model_assignments_for_sources("openai_account", &requested)
        .expect("list assignments");

    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].account_id, target);
    assert_eq!(overrides[0].primary_window_tokens, Some(250));
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].source_id, target);
    assert_eq!(assignments[0].model_slug, "gpt-large");
}

#[test]
fn quota_pool_list_queries_use_existing_index_ordering() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let kind_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_source_model_assignments_for_kind_sql()
        ),
        vec![Value::Text("openai_account".to_string())],
    );
    assert!(
        kind_details.iter().any(|detail| {
            detail.contains("idx_quota_source_model_assignments_source")
                || detail.contains("sqlite_autoindex_quota_source_model_assignments")
        }),
        "quota assignment kind list should use source/primary key index, got {kind_details:?}"
    );
    assert_no_temp_ordering(&kind_details, "quota assignment kind list");

    let source_id_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_assignment_source_ids_for_kind_sql()
        ),
        vec![Value::Text("openai_account".to_string())],
    );
    assert!(
        source_id_details.iter().any(|detail| {
            detail.contains("idx_quota_source_model_assignments_source")
                || detail.contains("sqlite_autoindex_quota_source_model_assignments")
        }),
        "quota assignment source id list should use source/primary key index, got {source_id_details:?}"
    );
    assert_no_temp_ordering(&source_id_details, "quota assignment source id list");

    let source_model_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_assignment_models_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-a".to_string()),
        ],
    );
    assert!(
        source_model_details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_quota_source_model_assignments")),
        "quota assignment source model list should use primary key index, got {source_model_details:?}"
    );
    assert_no_temp_ordering(&source_model_details, "quota assignment source model list");

    let template_details = collect_query_plan_details(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            account_quota_capacity_templates_list_sql()
        ),
    );
    assert_no_temp_ordering(&template_details, "account quota capacity template list");

    let override_details = collect_query_plan_details(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            account_quota_capacity_overrides_list_sql()
        ),
    );
    assert_no_temp_ordering(&override_details, "account quota capacity override list");
}

#[test]
fn quota_pool_delete_helpers_use_existing_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let assignment_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_quota_source_model_assignments_for_source_sql()
        ),
        vec![
            Value::Text("openai_account".to_string()),
            Value::Text("acc-a".to_string()),
        ],
    );
    assert!(
        assignment_details.iter().any(|detail| {
            detail.contains("idx_quota_source_model_assignments_source")
                || detail.contains("sqlite_autoindex_quota_source_model_assignments")
        }),
        "quota assignment source cleanup should use source/primary key index, got {assignment_details:?}"
    );

    let override_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_account_quota_capacity_override_sql()
        ),
        vec![Value::Text("acc-a".to_string())],
    );
    assert!(
        override_details
            .iter()
            .any(|detail| detail.contains("sqlite_autoindex_account_quota_capacity_overrides")),
        "quota capacity override delete should use account primary key index, got {override_details:?}"
    );
}

#[test]
fn quota_helper_chunk_queries_defer_final_ordering_to_rust() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let Some((assignment_condition, assignment_params)) =
        text_id_in_clause("source_id", &["acc-a".to_string(), "acc-b".to_string()])
    else {
        panic!("expected assignment condition");
    };
    let mut assignment_values = vec![Value::Text("openai_account".to_string())];
    assignment_values.extend(assignment_params);
    let assignment_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            quota_source_model_assignments_for_sources_chunk_sql(&assignment_condition)
        ),
        assignment_values,
    );
    let Some((override_condition, override_params)) =
        text_id_in_clause("account_id", &["acc-a".to_string(), "acc-b".to_string()])
    else {
        panic!("expected override condition");
    };
    let override_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            account_quota_capacity_overrides_for_accounts_chunk_sql(&override_condition)
        ),
        override_params,
    );

    assert!(
        assignment_details.iter().any(|detail| {
            detail.contains("idx_quota_source_model_assignments_source")
                || detail.contains("sqlite_autoindex_quota_source_model_assignments")
        }),
        "quota assignment chunk query should use source lookup index, got {assignment_details:?}"
    );
    assert!(
        override_details
            .iter()
            .any(|detail| { detail.contains("sqlite_autoindex_account_quota_capacity_overrides") }),
        "quota override chunk query should use account lookup index, got {override_details:?}"
    );
    assert_no_temp_ordering(&assignment_details, "quota assignment chunk query");
    assert_no_temp_ordering(&override_details, "quota override chunk query");
}
