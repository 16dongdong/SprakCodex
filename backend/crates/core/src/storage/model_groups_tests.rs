use super::*;
use crate::storage::{AppUser, ModelCatalogModelRecord};

fn model_catalog_record(slug: &str) -> ModelCatalogModelRecord {
    ModelCatalogModelRecord {
        scope: "default".to_string(),
        slug: slug.to_string(),
        display_name: slug.to_string(),
        source_kind: "remote".to_string(),
        user_edited: false,
        description: None,
        default_reasoning_level: None,
        shell_type: None,
        visibility: Some("list".to_string()),
        supported_in_api: Some(true),
        priority: Some(0),
        availability_nux_json: None,
        upgrade_json: None,
        base_instructions: None,
        model_messages_json: None,
        supports_reasoning_summaries: None,
        default_reasoning_summary: None,
        support_verbosity: None,
        default_verbosity_json: None,
        apply_patch_tool_type: None,
        web_search_tool_type: None,
        truncation_mode: None,
        truncation_limit: None,
        truncation_extra_json: None,
        supports_parallel_tool_calls: None,
        supports_image_detail_original: None,
        context_window: None,
        auto_compact_token_limit: None,
        effective_context_window_percent: None,
        minimal_client_version_json: None,
        supports_search_tool: None,
        extra_json: "{}".to_string(),
        sort_index: 0,
        updated_at: now_ts(),
    }
}

fn collect_query_plan(storage: &Storage, sql: &str) -> String {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }
    plan
}

#[test]
fn resolves_lowest_effective_model_group_rate() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .insert_app_user(&AppUser {
            id: "usr_1".to_string(),
            username: "member".to_string(),
            display_name: None,
            password_hash: "hash".to_string(),
            role: "member".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
        })
        .expect("insert user");
    storage
        .upsert_model_catalog_models(&[model_catalog_record("gpt-test")])
        .expect("seed catalog");
    storage
        .upsert_model_group(&ModelGroup {
            id: "mg_a".to_string(),
            name: "A".to_string(),
            description: None,
            status: "active".to_string(),
            sort: 0,
            is_default: false,
            rate_multiplier_millis: 1500,
            created_at: now,
            updated_at: now,
        })
        .expect("save group a");
    storage
        .upsert_model_group(&ModelGroup {
            id: "mg_b".to_string(),
            name: "B".to_string(),
            description: None,
            status: "active".to_string(),
            sort: 1,
            is_default: false,
            rate_multiplier_millis: 900,
            created_at: now,
            updated_at: now,
        })
        .expect("save group b");
    storage
        .replace_model_group_models(
            "mg_a",
            &[ModelGroupModel {
                group_id: "mg_a".to_string(),
                platform_model_slug: "gpt-test".to_string(),
                enabled: true,
                rate_multiplier_millis: Some(1000),
                billing_model_slug: None,
                note: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("models a");
    storage
        .replace_model_group_models(
            "mg_b",
            &[ModelGroupModel {
                group_id: "mg_b".to_string(),
                platform_model_slug: "gpt-test".to_string(),
                enabled: true,
                rate_multiplier_millis: Some(1200),
                billing_model_slug: Some("gpt-bill".to_string()),
                note: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("models b");
    storage
        .replace_user_model_groups_for_group(
            "mg_a",
            &[UserModelGroup {
                user_id: "usr_1".to_string(),
                group_id: "mg_a".to_string(),
                status: "active".to_string(),
                expires_at: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("assign a");
    storage
        .replace_user_model_groups_for_group(
            "mg_b",
            &[UserModelGroup {
                user_id: "usr_1".to_string(),
                group_id: "mg_b".to_string(),
                status: "active".to_string(),
                expires_at: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("assign b");

    let access = storage
        .resolve_model_group_access_for_user("usr_1", "gpt-test", now)
        .expect("resolve access")
        .expect("access");

    assert_eq!(access.group_id, "mg_b");
    assert_eq!(access.rate_multiplier_millis, 1080);
    assert_eq!(access.billing_model_slug.as_deref(), Some("gpt-bill"));
}

#[test]
fn pruning_default_group_models_keeps_rows_when_catalog_is_empty() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .replace_model_group_models(
            DEFAULT_MODEL_GROUP_ID,
            &[ModelGroupModel {
                group_id: DEFAULT_MODEL_GROUP_ID.to_string(),
                platform_model_slug: "gpt-existing".to_string(),
                enabled: true,
                rate_multiplier_millis: None,
                billing_model_slug: None,
                note: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("seed default group model");

    storage
        .prune_default_model_group_models_not_in_catalog()
        .expect("prune default group models");

    let slugs = storage
        .list_model_group_models_for_group(DEFAULT_MODEL_GROUP_ID)
        .expect("list default group models")
        .into_iter()
        .map(|model| model.platform_model_slug)
        .collect::<Vec<_>>();
    assert_eq!(slugs, vec!["gpt-existing"]);
}

#[test]
fn model_group_list_snapshot_bootstraps_and_loads_related_rows() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .insert_app_user(&AppUser {
            id: "usr_snapshot".to_string(),
            username: "snapshot-member".to_string(),
            display_name: None,
            password_hash: "hash".to_string(),
            role: "member".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
        })
        .expect("insert user");
    storage
        .upsert_model_catalog_models(&[model_catalog_record("gpt-snapshot")])
        .expect("seed catalog");

    let snapshot = storage
        .load_model_group_list_snapshot()
        .expect("load model group snapshot");

    assert!(snapshot
        .groups
        .iter()
        .any(|group| group.id == DEFAULT_MODEL_GROUP_ID));
    assert!(snapshot.models.iter().any(|model| {
        model.group_id == DEFAULT_MODEL_GROUP_ID && model.platform_model_slug == "gpt-snapshot"
    }));
    assert!(snapshot.user_assignments.iter().any(|assignment| {
        assignment.user_id == "usr_snapshot" && assignment.group_id == DEFAULT_MODEL_GROUP_ID
    }));
}

#[test]
fn list_model_groups_uses_list_order_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", model_group_list_sql()),
    );
    let default_plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", default_model_group_id_sql()),
    );
    let by_id_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_group_by_id_sql().replace("?1", "'mg_default'")
        ),
    );

    assert!(
        plan.contains("idx_model_groups_list_order"),
        "expected model group list-order index in plan, got {plan}"
    );
    assert!(
        default_plan.contains("idx_model_groups_default"),
        "expected default model group lookup to use default-group index, got {default_plan}"
    );
    assert!(
        by_id_plan.contains("sqlite_autoindex_model_groups_1"),
        "expected model group direct lookup to use primary-key index, got {by_id_plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "model group list query should avoid temp sorting, got {plan}"
    );
}

#[test]
fn model_group_model_lists_use_primary_key_ordering() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let list_plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", model_group_models_list_sql()),
    );
    let group_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_group_models_for_group_sql().replace("?1", "'mg_default'")
        ),
    );

    for (label, plan) in [
        ("global model-group model list", list_plan),
        ("per-group model list", group_plan),
    ] {
        assert!(
            plan.contains("sqlite_autoindex_model_group_models_1"),
            "expected {label} to use model-group model primary-key order, got {plan}"
        );
        assert!(
            !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
            "expected {label} to avoid temp sorting, got {plan}"
        );
    }
}

#[test]
fn user_model_group_lists_use_primary_key_ordering() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let list_plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", user_model_groups_list_sql()),
    );
    let user_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            user_model_groups_for_user_sql().replace("?1", "'usr_default'")
        ),
    );

    for (label, plan) in [
        ("global user model-group assignment list", list_plan),
        ("per-user model-group assignment list", user_plan),
    ] {
        assert!(
            plan.contains("sqlite_autoindex_user_model_groups_1"),
            "expected {label} to use user model-group primary-key order, got {plan}"
        );
        assert!(
            !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
            "expected {label} to avoid temp sorting, got {plan}"
        );
    }
}

#[test]
fn member_access_queries_use_existing_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let allowed_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            allowed_model_slugs_for_user_sql()
                .replace("?1", "'usr_default'")
                .replace("?2", "1700000000")
        ),
    );
    let access_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_group_access_for_user_sql()
                .replace("?1", "'usr_default'")
                .replace("?2", "'gpt-test'")
                .replace("?3", "1700000000")
        ),
    );

    for (label, plan) in [
        ("allowed model slugs", &allowed_plan),
        ("model group access", &access_plan),
    ] {
        assert!(
            plan.contains("idx_user_model_groups_user_status"),
            "expected {label} to use user/status assignment lookup, got {plan}"
        );
        assert!(
            plan.contains("sqlite_autoindex_model_groups_1"),
            "expected {label} to use model group primary-key lookup, got {plan}"
        );
        assert!(
            plan.contains("sqlite_autoindex_model_group_models_1"),
            "expected {label} to use model-group model primary-key lookup, got {plan}"
        );
        assert!(
            plan.contains("idx_model_catalog_models_scope_supported_in_api")
                || plan.contains("sqlite_autoindex_model_catalog_models_1"),
            "expected {label} to use a model catalog lookup index, got {plan}"
        );
    }

    assert!(
        !allowed_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "allowed model slug query should read in model order without temp sorting, got {allowed_plan}"
    );
}
#[test]
fn model_group_write_delete_helpers_use_existing_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let group_models_delete_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_group_models_for_group_sql().replace("?1", "'mg_default'")
        ),
    );
    let platform_model_delete_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_model_group_models_for_platform_model_sql().replace("?1", "'gpt-test'")
        ),
    );
    let group_delete_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_non_default_model_group_by_id_sql().replace("?1", "'mg_custom'")
        ),
    );
    let default_prune_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            prune_default_model_group_models_not_in_catalog_sql()
        ),
    );

    assert!(
        group_models_delete_plan.contains("sqlite_autoindex_model_group_models_1"),
        "expected group-scoped model delete to use model-group model primary-key index, got {group_models_delete_plan}"
    );
    assert!(
        platform_model_delete_plan.contains("idx_model_group_models_model"),
        "expected platform-model delete to use platform model lookup index, got {platform_model_delete_plan}"
    );
    assert!(
        group_delete_plan.contains("sqlite_autoindex_model_groups_1"),
        "expected non-default model group delete to use model group primary-key index, got {group_delete_plan}"
    );
    assert!(
        default_prune_plan.contains("sqlite_autoindex_model_group_models_1"),
        "expected default model prune to use model-group model primary-key index, got {default_prune_plan}"
    );
    assert!(
        default_prune_plan.contains("idx_model_groups_default")
            || default_prune_plan.contains("sqlite_autoindex_model_groups_1"),
        "expected default model prune to use default-group lookup, got {default_prune_plan}"
    );
    assert!(
        default_prune_plan.contains("idx_model_catalog_models_scope_supported_in_api")
            || default_prune_plan.contains("sqlite_autoindex_model_catalog_models_1"),
        "expected default model prune to use model catalog lookup, got {default_prune_plan}"
    );
}

#[test]
fn replace_user_model_groups_for_group_uses_group_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_user_model_groups_for_group_sql().replace("?1", "'mg_default'")
        ),
    );

    assert!(
        plan.contains("idx_user_model_groups_group_lookup"),
        "expected user model-group assignment delete to use group lookup index, got {plan}"
    );
}

#[test]
fn member_access_queries_ignore_models_missing_from_catalog() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .insert_app_user(&AppUser {
            id: "usr_1".to_string(),
            username: "member".to_string(),
            display_name: None,
            password_hash: "hash".to_string(),
            role: "member".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
        })
        .expect("insert user");
    storage
        .upsert_model_catalog_models(&[model_catalog_record("gpt-current")])
        .expect("seed catalog");
    storage
        .replace_model_group_models(
            DEFAULT_MODEL_GROUP_ID,
            &[
                ModelGroupModel {
                    group_id: DEFAULT_MODEL_GROUP_ID.to_string(),
                    platform_model_slug: "gpt-current".to_string(),
                    enabled: true,
                    rate_multiplier_millis: None,
                    billing_model_slug: None,
                    note: None,
                    created_at: now,
                    updated_at: now,
                },
                ModelGroupModel {
                    group_id: DEFAULT_MODEL_GROUP_ID.to_string(),
                    platform_model_slug: "gpt-stale".to_string(),
                    enabled: true,
                    rate_multiplier_millis: None,
                    billing_model_slug: None,
                    note: None,
                    created_at: now,
                    updated_at: now,
                },
            ],
        )
        .expect("seed group models");
    storage
        .replace_user_model_groups_for_group(
            DEFAULT_MODEL_GROUP_ID,
            &[UserModelGroup {
                user_id: "usr_1".to_string(),
                group_id: DEFAULT_MODEL_GROUP_ID.to_string(),
                status: "active".to_string(),
                expires_at: None,
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("assign default group");

    let slugs = storage
        .allowed_model_slugs_for_user("usr_1", now)
        .expect("read allowed slugs");
    let stale_access = storage
        .resolve_model_group_access_for_user("usr_1", "gpt-stale", now)
        .expect("resolve stale access");

    assert_eq!(slugs, vec!["gpt-current"]);
    assert!(stale_access.is_none());
}
