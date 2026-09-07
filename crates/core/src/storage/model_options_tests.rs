use super::*;

fn model_record(slug: &str) -> ModelCatalogModelRecord {
    ModelCatalogModelRecord {
        scope: "default".to_string(),
        slug: slug.to_string(),
        display_name: slug.to_string(),
        source_kind: "remote".to_string(),
        visibility: Some("list".to_string()),
        supported_in_api: Some(true),
        extra_json: "{}".to_string(),
        ..ModelCatalogModelRecord::default()
    }
}

fn collect_query_plan_details(storage: &Storage, sql: &str) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    collect_query_plan_rows(&mut rows)
}

fn collect_query_plan_details_with_params(
    storage: &Storage,
    sql: &str,
    params: Vec<SqlValue>,
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

fn assert_no_temp_ordering(details: &[String], label: &str) {
    assert!(
        !details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "{label} should avoid temp ORDER BY sorting, got {details:?}"
    );
}

#[test]
fn count_available_model_catalog_models_uses_visibility_and_api_filters() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut hidden = model_record("hidden-model");
    hidden.visibility = Some("hide".to_string());
    let mut remote_hidden = model_record("remote-hidden-model");
    remote_hidden.visibility = Some("hidden".to_string());
    let mut unsupported = model_record("unsupported-model");
    unsupported.supported_in_api = Some(false);
    let mut default_supported = model_record("default-supported-model");
    default_supported.supported_in_api = None;
    storage
        .upsert_model_catalog_models(&[
            model_record("available-model"),
            hidden,
            remote_hidden,
            unsupported,
            default_supported,
        ])
        .expect("seed models");

    assert_eq!(
        storage
            .count_available_model_catalog_models("default")
            .expect("count models"),
        2
    );

    let sql = count_available_model_catalog_models_sql();
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {sql}"),
        vec![SqlValue::Text("default".to_string())],
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("model_catalog_models") && detail.contains("index")),
        "available catalog model count should use a model catalog index, got {details:?}"
    );
}

#[test]
fn model_catalog_model_exists_checks_single_slug() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    storage
        .upsert_model_catalog_models(&[model_record("available-model")])
        .expect("seed model");

    assert!(storage
        .model_catalog_model_exists("default", "available-model")
        .expect("check existing model"));
    assert!(!storage
        .model_catalog_model_exists("default", "missing-model")
        .expect("check missing model"));
}

#[test]
fn list_existing_model_catalog_slugs_filters_requested_slugs() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    storage
        .upsert_model_catalog_models(&[
            model_record("available-a"),
            model_record("available-b"),
            model_record("unrequested"),
        ])
        .expect("seed models");

    let slugs = storage
        .list_existing_model_catalog_slugs(
            "default",
            &[
                "available-b".to_string(),
                "missing".to_string(),
                "available-a".to_string(),
                "available-a".to_string(),
            ],
        )
        .expect("list existing slugs");

    assert_eq!(slugs, vec!["available-a", "available-b"]);
}

#[test]
fn model_catalog_slug_chunk_queries_defer_final_ordering_to_rust() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let existing_sql =
        existing_model_catalog_slugs_chunk_sql("slug IN ('available-a', 'available-b')");
    let existing_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {existing_sql}"),
        vec![SqlValue::Text("default".to_string())],
    );
    let remote_sql = remote_unedited_model_catalog_models_for_slugs_chunk_sql(
        "slug IN ('available-a', 'available-b')",
    );
    let remote_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {remote_sql}"),
        vec![SqlValue::Text("default".to_string())],
    );

    assert!(
        existing_details
            .iter()
            .any(|detail| detail.contains("model_catalog_models") && detail.contains("index")),
        "existing slug chunk query should use a model catalog lookup index, got {existing_details:?}"
    );
    assert!(
        remote_details
            .iter()
            .any(|detail| detail.contains("model_catalog_models") && detail.contains("index")),
        "remote catalog chunk query should use a model catalog lookup index, got {remote_details:?}"
    );
    assert!(
        !existing_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "existing slug chunk query should avoid per-chunk ORDER BY temp sorting, got {existing_details:?}"
    );
    assert!(
        !remote_details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "remote catalog chunk query should avoid per-chunk ORDER BY temp sorting, got {remote_details:?}"
    );
}

#[test]
fn list_remote_unedited_model_catalog_models_for_slugs_preserves_catalog_order() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut later = model_record("later");
    later.sort_index = 2;
    later.updated_at = 10;
    let mut first = model_record("first");
    first.sort_index = 1;
    first.updated_at = 5;
    let mut newer_same_sort = model_record("newer-same-sort");
    newer_same_sort.sort_index = 1;
    newer_same_sort.updated_at = 20;
    storage
        .upsert_model_catalog_models(&[later, first, newer_same_sort])
        .expect("seed models");

    let rows = storage
        .list_remote_unedited_model_catalog_models_for_slugs(
            "default",
            &[
                "later".to_string(),
                "first".to_string(),
                "newer-same-sort".to_string(),
            ],
        )
        .expect("list remote catalog models");

    assert_eq!(
        rows.into_iter().map(|row| row.slug).collect::<Vec<_>>(),
        vec![
            "newer-same-sort".to_string(),
            "first".to_string(),
            "later".to_string()
        ]
    );
}

#[test]
fn list_remote_unedited_model_catalog_slugs_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut user_edited = model_record("user-edited");
    user_edited.user_edited = true;
    let mut custom = model_record("custom-model");
    custom.source_kind = "custom".to_string();
    storage
        .upsert_model_catalog_models(&[
            model_record("remote-a"),
            user_edited,
            custom,
            model_record("remote-b"),
        ])
        .expect("seed models");

    let slugs = storage
        .list_remote_unedited_model_catalog_slugs("default")
        .expect("list remote unedited slugs");

    assert_eq!(slugs, vec!["remote-a", "remote-b"]);
}

#[test]
fn list_api_available_model_catalog_slugs_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut hidden = model_record("hidden-model");
    hidden.visibility = Some("hidden".to_string());
    let mut hide = model_record("hide-model");
    hide.visibility = Some("hide".to_string());
    let mut disabled = model_record("disabled-model");
    disabled.visibility = Some("disabled".to_string());
    let mut unavailable = model_record("unavailable-model");
    unavailable.visibility = Some("unavailable".to_string());
    let mut unsupported = model_record("unsupported-model");
    unsupported.supported_in_api = Some(false);
    let mut default_supported = model_record("default-supported-model");
    default_supported.supported_in_api = None;
    default_supported.sort_index = 1;
    let mut available = model_record("available-model");
    available.sort_index = 0;
    storage
        .upsert_model_catalog_models(&[
            default_supported,
            hidden,
            hide,
            unsupported,
            disabled,
            unavailable,
            available,
        ])
        .expect("seed models");

    let slugs = storage
        .list_api_available_model_catalog_slugs("default")
        .expect("list api available slugs");

    assert_eq!(slugs, vec!["available-model", "default-supported-model"]);
}

#[test]
fn find_first_api_available_model_catalog_slug_uses_catalog_order() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut hidden = model_record("hidden-model");
    hidden.visibility = Some("hidden".to_string());
    hidden.sort_index = 0;
    let mut unsupported = model_record("unsupported-model");
    unsupported.supported_in_api = Some(false);
    unsupported.sort_index = 1;
    let mut selected = model_record("selected-model");
    selected.sort_index = 2;
    let mut later = model_record("later-model");
    later.sort_index = 3;
    storage
        .upsert_model_catalog_models(&[hidden, unsupported, later, selected])
        .expect("seed models");

    let slug = storage
        .find_first_api_available_model_catalog_slug("default")
        .expect("find first api available slug");

    assert_eq!(slug.as_deref(), Some("selected-model"));
}

#[test]
fn list_api_available_model_catalog_slugs_with_prefix_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut gpt = model_record("gpt-available");
    gpt.sort_index = 1;
    let mut codex = model_record("gpt-codex");
    codex.sort_index = 0;
    let other = model_record("claude-available");
    let mut hidden = model_record("gpt-hidden");
    hidden.visibility = Some("hide".to_string());
    storage
        .upsert_model_catalog_models(&[gpt, codex, other, hidden])
        .expect("seed models");

    let slugs = storage
        .list_api_available_model_catalog_slugs_with_prefix("default", " GPT-")
        .expect("list prefixed api available slugs");

    assert_eq!(slugs, vec!["gpt-codex", "gpt-available"]);
}

#[test]
fn delete_model_catalog_string_item_kinds_deletes_selected_kinds_for_slug() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();
    storage
        .upsert_model_catalog_additional_speed_tiers(&[
            ModelCatalogStringItemRecord {
                scope: "default".to_string(),
                slug: "target-model".to_string(),
                value: "fast".to_string(),
                sort_index: 0,
                updated_at: now,
            },
            ModelCatalogStringItemRecord {
                scope: "default".to_string(),
                slug: "other-model".to_string(),
                value: "fast".to_string(),
                sort_index: 0,
                updated_at: now,
            },
        ])
        .expect("seed speed tiers");
    storage
        .upsert_model_catalog_experimental_supported_tools(&[ModelCatalogStringItemRecord {
            scope: "default".to_string(),
            slug: "target-model".to_string(),
            value: "web_search".to_string(),
            sort_index: 0,
            updated_at: now,
        }])
        .expect("seed tools");
    storage
        .upsert_model_catalog_input_modalities(&[ModelCatalogStringItemRecord {
            scope: "default".to_string(),
            slug: "target-model".to_string(),
            value: "text".to_string(),
            sort_index: 0,
            updated_at: now,
        }])
        .expect("seed input modalities");

    storage
        .delete_model_catalog_string_item_kinds(
            "default",
            "target-model",
            &["additional_speed_tiers", "experimental_supported_tools"],
        )
        .expect("delete selected string item kinds");

    assert!(storage
        .list_model_catalog_additional_speed_tiers("default")
        .expect("list speed tiers")
        .iter()
        .all(|item| item.slug != "target-model"));
    assert!(storage
        .list_model_catalog_experimental_supported_tools("default")
        .expect("list tools")
        .iter()
        .all(|item| item.slug != "target-model"));
    assert_eq!(
        storage
            .list_model_catalog_input_modalities("default")
            .expect("list input modalities")
            .into_iter()
            .map(|item| item.slug)
            .collect::<Vec<_>>(),
        vec!["target-model".to_string()]
    );
    assert_eq!(
        storage
            .list_model_catalog_additional_speed_tiers("default")
            .expect("list remaining speed tiers")
            .into_iter()
            .map(|item| item.slug)
            .collect::<Vec<_>>(),
        vec!["other-model".to_string()]
    );
}

#[test]
fn list_model_catalog_models_uses_scope_order_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let sql = model_catalog_models_for_scope_sql();
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {sql}"),
        vec![SqlValue::Text("default".to_string())],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_catalog_models_scope_order")),
        "expected model catalog scope order index, got {details:?}"
    );
}

#[test]
fn model_catalog_ordered_slug_queries_use_scope_order_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let remote_sql = model_catalog_ordered_slug_sql(
        &["source_kind = 'remote'", "COALESCE(user_edited, 0) = 0"],
        None,
    );
    let api_sql = model_catalog_ordered_slug_sql(&[MODEL_CATALOG_API_AVAILABLE_CONDITION], None);
    let first_api_sql =
        model_catalog_ordered_slug_sql(&[MODEL_CATALOG_API_AVAILABLE_CONDITION], Some(1));
    let prefixed_api_sql = model_catalog_ordered_slug_sql(
        &[
            MODEL_CATALOG_API_AVAILABLE_CONDITION,
            "LOWER(TRIM(slug)) LIKE ?2",
        ],
        None,
    );

    for (label, sql, params) in [
        (
            "remote unedited catalog slug list",
            format!("EXPLAIN QUERY PLAN {remote_sql}"),
            vec![SqlValue::Text("default".to_string())],
        ),
        (
            "API available catalog slug list",
            format!("EXPLAIN QUERY PLAN {api_sql}"),
            vec![SqlValue::Text("default".to_string())],
        ),
        (
            "first API available catalog slug",
            format!("EXPLAIN QUERY PLAN {first_api_sql}"),
            vec![SqlValue::Text("default".to_string())],
        ),
        (
            "prefixed API available catalog slug list",
            format!("EXPLAIN QUERY PLAN {prefixed_api_sql}"),
            vec![
                SqlValue::Text("default".to_string()),
                SqlValue::Text("gpt-%".to_string()),
            ],
        ),
    ] {
        let details = collect_query_plan_details_with_params(&storage, &sql, params);
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("idx_model_catalog_models_scope_order")),
            "{label} should use model catalog scope order index, got {details:?}"
        );
        assert_no_temp_ordering(&details, label);
    }
}

#[test]
fn model_catalog_child_list_queries_use_existing_order_indexes() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let reasoning_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_catalog_reasoning_levels_list_sql()
        ),
        vec![SqlValue::Text("default".to_string())],
    );
    assert!(
        reasoning_details
            .iter()
            .any(|detail| detail.contains("idx_model_catalog_reasoning_levels_scope_sort")),
        "reasoning level list should use scope sort index, got {reasoning_details:?}"
    );
    assert_no_temp_ordering(&reasoning_details, "reasoning level list");

    let string_item_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_catalog_string_items_list_sql()
        ),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text(STRING_ITEM_KIND_INPUT_MODALITIES.to_string()),
        ],
    );
    assert!(
        string_item_details
            .iter()
            .any(|detail| detail.contains("idx_model_catalog_string_items_scope_kind_sort")),
        "string item list should use scope kind sort index, got {string_item_details:?}"
    );
    assert_no_temp_ordering(&string_item_details, "string item list");

    let multi_kind_sql = model_catalog_string_items_for_kinds_sql("?2, ?3");
    let multi_kind_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {multi_kind_sql}"),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text(STRING_ITEM_KIND_INPUT_MODALITIES.to_string()),
            SqlValue::Text(STRING_ITEM_KIND_AVAILABLE_IN_PLANS.to_string()),
        ],
    );
    assert!(
        multi_kind_details
            .iter()
            .any(|detail| detail.contains("idx_model_catalog_string_items_scope_kind_sort")),
        "multi-kind string item list should use scope kind sort index, got {multi_kind_details:?}"
    );
    assert_no_temp_ordering(&multi_kind_details, "multi-kind string item list");
}

#[test]
fn model_catalog_model_exists_uses_primary_key_lookup() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let details = collect_query_plan_details(
        &storage,
        "EXPLAIN QUERY PLAN
         SELECT 1
         FROM model_catalog_models
         WHERE scope = 'default'
           AND slug = 'available-model'
         LIMIT 1",
    );

    assert!(details
        .iter()
        .any(|detail| detail.contains("search model_catalog_models") && detail.contains("index")));
}

#[test]
fn model_catalog_delete_helpers_use_existing_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let model_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", delete_model_catalog_model_sql()),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text("gpt-test".to_string()),
        ],
    );
    let reasoning_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_catalog_reasoning_levels_sql()
        ),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text("gpt-test".to_string()),
        ],
    );
    let string_item_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_catalog_string_items_sql()
        ),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text("gpt-test".to_string()),
            SqlValue::Text(STRING_ITEM_KIND_INPUT_MODALITIES.to_string()),
        ],
    );
    let string_item_kind_sql = delete_model_catalog_string_item_kinds_sql(
        "item_kind IN ('input_modalities', 'available_in_plans')",
    );
    let string_item_kind_delete_details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {string_item_kind_sql}"),
        vec![
            SqlValue::Text("default".to_string()),
            SqlValue::Text("gpt-test".to_string()),
        ],
    );

    assert!(
        model_delete_details.iter().any(|detail| {
            detail.contains("model_catalog_models") && detail.contains("index")
        }),
        "model catalog model delete should use a model catalog lookup index, got {model_delete_details:?}"
    );
    assert!(
        reasoning_delete_details.iter().any(|detail| {
            detail.contains("idx_model_catalog_reasoning_levels_scope_sort")
                || detail.contains("sqlite_autoindex_model_catalog_reasoning_levels_1")
        }),
        "reasoning level delete should use an existing reasoning-level lookup index, got {reasoning_delete_details:?}"
    );
    for (label, details) in [
        (
            "single string item kind delete",
            &string_item_delete_details,
        ),
        (
            "multi string item kind delete",
            &string_item_kind_delete_details,
        ),
    ] {
        assert!(
            details.iter().any(|detail| {
                detail.contains("sqlite_autoindex_model_catalog_string_items_1")
                    || detail.contains("idx_model_catalog_string_items_scope_kind_sort")
            }),
            "{label} should use an existing string item lookup index, got {details:?}"
        );
    }
}

#[test]
fn model_catalog_scope_lookup_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", model_catalog_scope_by_scope_sql()),
        vec![SqlValue::Text("default".to_string())],
    );

    assert!(
        details.iter().any(|detail| {
            detail.contains("search model_catalog_scopes") && detail.contains("index")
        }),
        "scope lookup should use the model catalog scope primary-key index, got {details:?}"
    );
}
