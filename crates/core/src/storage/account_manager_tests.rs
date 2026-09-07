use super::super::Storage;
use super::super::{ApiKey, ApiKeyOwner, BillingRule};
use super::{
    active_admin_count_sql, active_app_session_by_token_hash_sql,
    active_app_session_user_with_wallet_sql, active_billing_rules_sql, api_key_owner_chunk_sql,
    api_key_owner_rows_sql, app_user_access_summary_by_id_sql, app_user_access_summary_sql,
    app_user_by_id_sql, app_user_by_username_sql, app_user_exists_sql, app_user_list_sql,
    app_username_exists_sql, app_wallet_by_owner_sql, dashboard_app_user_summary_sql,
    delete_api_key_owners_for_user_sql, delete_app_user_by_id_sql,
    delete_app_user_sessions_for_user_sql, delete_app_wallet_ledger_entries_for_user_wallets_sql,
    delete_app_wallets_for_user_sql, delete_billing_rule_by_id_sql,
    delete_user_model_groups_for_user_sql, member_app_user_count_sql,
    public_app_user_with_wallet_sql, request_charge_ledger_entry_count_sql,
    revoke_app_user_session_by_token_hash_sql, touch_app_user_session_sql,
    update_app_user_display_name_sql, update_app_user_last_login_sql,
    update_app_user_password_hash_sql, update_app_user_role_sql, update_app_user_status_sql,
    user_api_key_ids_for_user_sql, user_wallet_available_credit_sql,
    user_wallets_for_users_chunk_sql,
};
use rusqlite::{params_from_iter, types::Value};

fn seed_api_key(storage: &Storage, key_id: &str) {
    storage
        .insert_api_key(&ApiKey {
            id: key_id.to_string(),
            name: Some(key_id.to_string()),
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
            key_hash: format!("hash-{key_id}"),
            status: "active".to_string(),
            created_at: 1,
            last_used_at: None,
        })
        .expect("seed api key");
}

fn seed_app_user(storage: &Storage, user_id: &str) {
    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES (?1, ?2, NULL, 'hash', 'member', 'active', 1, 1, NULL)",
            (user_id, format!("{user_id}@example.com")),
        )
        .expect("seed app user");
}

fn seed_app_project(storage: &Storage, project_id: &str, owner_user_id: &str) {
    storage
        .conn
        .execute(
            "INSERT INTO app_projects (
                id, name, owner_user_id, status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'active', 1, 1)",
            (project_id, project_id, owner_user_id),
        )
        .expect("seed app project");
}

fn collect_query_plan(storage: &Storage, sql: &str) -> String {
    collect_query_plan_with_params(storage, sql, Vec::new())
}

fn collect_query_plan_with_params(storage: &Storage, sql: &str, params: Vec<Value>) -> String {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query(params_from_iter(params)).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }
    plan
}

#[test]
fn active_billing_rules_query_uses_order_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            active_billing_rules_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query([100_i64]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_billing_rules_active_order"),
        "expected active billing rules order index in plan, got {plan}"
    );
}

#[test]
fn billing_rule_owner_delete_paths_use_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (sql, expected_index) in [
        (
            "EXPLAIN QUERY PLAN DELETE FROM billing_rules WHERE user_id = 'user-1'",
            "idx_billing_rules_user_lookup",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM billing_rules WHERE project_id = 'project-1'",
            "idx_billing_rules_project_lookup",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM billing_rules WHERE api_key_id = 'key-1'",
            "idx_billing_rules_api_key_lookup",
        ),
    ] {
        let plan = collect_query_plan(&storage, sql);
        assert!(
            plan.contains(expected_index),
            "expected billing rule delete path to use {expected_index}, got {plan}"
        );
    }
}

#[test]
fn redeem_record_foreign_key_paths_use_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (sql, expected_index) in [
        (
            "EXPLAIN QUERY PLAN DELETE FROM redeem_records WHERE code_id = 'code-1'",
            "idx_redeem_records_code_lookup",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM redeem_records WHERE wallet_id = 'wallet-1'",
            "idx_redeem_records_wallet_lookup",
        ),
        (
            "EXPLAIN QUERY PLAN UPDATE redeem_records SET ledger_entry_id = NULL WHERE ledger_entry_id = 'ledger-1'",
            "idx_redeem_records_ledger_entry_lookup",
        ),
    ] {
        let plan = collect_query_plan(&storage, sql);
        assert!(
            plan.contains(expected_index),
            "expected redeem record foreign-key path to use {expected_index}, got {plan}"
        );
    }
}

#[test]
fn created_by_user_delete_paths_use_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (sql, expected_index) in [
        (
            "EXPLAIN QUERY PLAN UPDATE app_wallet_ledger_entries SET created_by_user_id = NULL WHERE created_by_user_id = 'user-1'",
            "idx_app_wallet_ledger_created_by_lookup",
        ),
        (
            "EXPLAIN QUERY PLAN UPDATE redeem_code_batches SET created_by_user_id = NULL WHERE created_by_user_id = 'user-1'",
            "idx_redeem_code_batches_created_by_lookup",
        ),
    ] {
        let plan = collect_query_plan(&storage, sql);
        assert!(
            plan.contains(expected_index),
            "expected created-by user delete path to use {expected_index}, got {plan}"
        );
    }
}

#[test]
fn app_session_token_paths_use_token_hash_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            active_app_session_by_token_hash_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query(("token-hash", 100_i64)).expect("query explain");
    let mut active_session_plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        active_session_plan.push_str(&detail);
        active_session_plan.push('\n');
    }
    assert!(
        active_session_plan.contains("idx_app_user_sessions_token_hash")
            || active_session_plan.contains("sqlite_autoindex_app_user_sessions_2"),
        "expected active app session lookup to use token hash index, got {active_session_plan}"
    );

    let revoke_plan = collect_query_plan_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            revoke_app_user_session_by_token_hash_sql()
        ),
        vec![Value::Integer(100), Value::Text("token-hash".to_string())],
    );
    assert!(
        revoke_plan.contains("idx_app_user_sessions_token_hash")
            || revoke_plan.contains("sqlite_autoindex_app_user_sessions_2"),
        "expected app session token path to use token hash index, got {revoke_plan}"
    );
}

#[test]
fn app_user_write_helpers_use_primary_key_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (label, sql, params, expected_index) in [
        (
            "app user exists",
            app_user_exists_sql(),
            vec![Value::Text("user-1".to_string())],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user last login update",
            update_app_user_last_login_sql(),
            vec![Value::Integer(1), Value::Text("user-1".to_string())],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user status update",
            update_app_user_status_sql(),
            vec![
                Value::Text("active".to_string()),
                Value::Integer(1),
                Value::Text("user-1".to_string()),
            ],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user role update",
            update_app_user_role_sql(),
            vec![
                Value::Text("member".to_string()),
                Value::Integer(1),
                Value::Text("user-1".to_string()),
            ],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user display name update",
            update_app_user_display_name_sql(),
            vec![
                Value::Text("User 1".to_string()),
                Value::Integer(1),
                Value::Text("user-1".to_string()),
            ],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user password hash update",
            update_app_user_password_hash_sql(),
            vec![
                Value::Text("hash".to_string()),
                Value::Integer(1),
                Value::Text("user-1".to_string()),
            ],
            "sqlite_autoindex_app_users_1",
        ),
        (
            "app user session touch",
            touch_app_user_session_sql(),
            vec![Value::Integer(1), Value::Text("session-1".to_string())],
            "sqlite_autoindex_app_user_sessions_1",
        ),
        (
            "billing rule delete",
            delete_billing_rule_by_id_sql(),
            vec![Value::Text("rule-1".to_string())],
            "sqlite_autoindex_billing_rules_1",
        ),
    ] {
        let plan =
            collect_query_plan_with_params(&storage, &format!("EXPLAIN QUERY PLAN {sql}"), params);
        assert!(
            plan.contains(expected_index),
            "expected {label} to use {expected_index}, got {plan}"
        );
    }
}

#[test]
fn app_user_delete_dependent_paths_use_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let user_id = Value::Text("user-1".to_string());
    for (sql, expected_index) in [
        (
            delete_api_key_owners_for_user_sql(),
            "idx_api_key_owners_user_key_lookup",
        ),
        (
            delete_app_user_sessions_for_user_sql(),
            "idx_app_user_sessions_user_id",
        ),
        (
            delete_user_model_groups_for_user_sql(),
            "idx_user_model_groups_user_status",
        ),
        (
            delete_app_wallets_for_user_sql(),
            "sqlite_autoindex_app_wallets_2",
        ),
        (delete_app_user_by_id_sql(), "sqlite_autoindex_app_users_1"),
    ] {
        let plan = collect_query_plan_with_params(
            &storage,
            &format!("EXPLAIN QUERY PLAN {sql}"),
            vec![user_id.clone()],
        );
        assert!(
            plan.contains(expected_index),
            "expected app user dependent path to use {expected_index}, got {plan}"
        );
    }

    let ledger_plan = collect_query_plan_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            delete_app_wallet_ledger_entries_for_user_wallets_sql()
        ),
        vec![user_id],
    );
    assert!(
        ledger_plan.contains("idx_app_wallet_ledger_wallet_created"),
        "expected app wallet ledger cleanup to use wallet lookup index, got {ledger_plan}"
    );
    assert!(
        ledger_plan.contains("sqlite_autoindex_app_wallets_2"),
        "expected app wallet ledger cleanup to use owner lookup subquery, got {ledger_plan}"
    );
}

fn billing_rule(id: &str) -> BillingRule {
    BillingRule {
        id: id.to_string(),
        name: id.to_string(),
        status: "active".to_string(),
        priority: 1,
        multiplier_millis: 1000,
        model_pattern: None,
        service_tier: None,
        user_id: None,
        project_id: None,
        api_key_id: None,
        starts_at: None,
        ends_at: None,
        created_at: 1,
        updated_at: 1,
    }
}

#[test]
fn active_billing_rules_for_context_filters_scope_columns_in_sql() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO api_keys (id, name, key_hash, status, created_at, last_used_at)
             VALUES
                ('key-1', 'Key 1', 'hash-1', 'enabled', 1, NULL),
                ('key-2', 'Key 2', 'hash-2', 'enabled', 1, NULL)",
            [],
        )
        .expect("seed api keys");
    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'user-1@example.com', NULL, 'hash', 'member', 'active', 1, 1, NULL),
                ('user-2', 'user-2@example.com', NULL, 'hash', 'member', 'active', 1, 1, NULL)",
            [],
        )
        .expect("seed users");
    storage
        .conn
        .execute(
            "INSERT INTO app_projects (id, name, owner_user_id, status, created_at, updated_at)
             VALUES
                ('project-1', 'Project 1', 'user-1', 'active', 1, 1),
                ('project-2', 'Project 2', 'user-2', 'active', 1, 1)",
            [],
        )
        .expect("seed projects");

    let global = billing_rule("global");
    let mut matching = billing_rule("matching");
    matching.api_key_id = Some("key-1".to_string());
    matching.user_id = Some("user-1".to_string());
    matching.project_id = Some("project-1".to_string());
    matching.service_tier = Some("Premium".to_string());
    matching.priority = 10;
    let mut wrong_key = billing_rule("wrong-key");
    wrong_key.api_key_id = Some("key-2".to_string());
    let mut wrong_user = billing_rule("wrong-user");
    wrong_user.user_id = Some("user-2".to_string());
    let mut wrong_project = billing_rule("wrong-project");
    wrong_project.project_id = Some("project-2".to_string());
    let mut wrong_tier = billing_rule("wrong-tier");
    wrong_tier.service_tier = Some("standard".to_string());
    let mut expired = billing_rule("expired");
    expired.ends_at = Some(50);
    let mut disabled = billing_rule("disabled");
    disabled.status = "disabled".to_string();

    for rule in [
        &global,
        &matching,
        &wrong_key,
        &wrong_user,
        &wrong_project,
        &wrong_tier,
        &expired,
        &disabled,
    ] {
        storage.upsert_billing_rule(rule).expect("insert rule");
    }

    let rules = storage
        .list_active_billing_rules_for_context(
            100,
            "key-1",
            Some("user-1"),
            Some("project-1"),
            Some("premium"),
        )
        .expect("list context rules");
    let ids = rules.into_iter().map(|rule| rule.id).collect::<Vec<_>>();

    assert_eq!(ids, vec!["matching".to_string(), "global".to_string()]);
}

#[test]
fn active_billing_rules_for_request_candidates_prefilter_model_rules() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let global = billing_rule("global");
    let mut exact = billing_rule("exact");
    exact.model_pattern = Some("gpt-4.1".to_string());
    exact.priority = 8;
    exact.updated_at = 8;
    let mut prefix = billing_rule("prefix");
    prefix.model_pattern = Some("gpt-4".to_string());
    prefix.priority = 7;
    prefix.updated_at = 7;
    let mut wildcard = billing_rule("wildcard");
    wildcard.model_pattern = Some("gpt-*".to_string());
    wildcard.priority = 6;
    wildcard.updated_at = 6;
    let mut unrelated = billing_rule("unrelated");
    unrelated.model_pattern = Some("claude".to_string());
    unrelated.priority = 20;
    unrelated.updated_at = 20;

    for rule in [&global, &exact, &prefix, &wildcard, &unrelated] {
        storage.upsert_billing_rule(rule).expect("insert rule");
    }

    let rules = storage
        .list_active_billing_rules_for_request_candidate(
            100,
            "key-1",
            None,
            None,
            None,
            Some("gpt-4.1"),
        )
        .expect("list request candidates");
    let ids = rules.into_iter().map(|rule| rule.id).collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "exact".to_string(),
            "prefix".to_string(),
            "wildcard".to_string(),
            "global".to_string()
        ]
    );
}

#[test]
fn username_lookup_uses_lower_username_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", app_user_by_username_sql()),
            ["member@example.com"],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("idx_app_users_lower_username"),
        "expected lower username index in plan, got {plan}"
    );

    let exists_plan = collect_query_plan_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", app_username_exists_sql()),
        vec![Value::Text("member@example.com".to_string())],
    );

    assert!(
        exists_plan.contains("idx_app_users_lower_username"),
        "expected username exists helper to use lower username index, got {exists_plan}"
    );
}

#[test]
fn app_user_id_lookup_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", app_user_by_id_sql()),
            ["user-1"],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("sqlite_autoindex_app_users"),
        "expected app user id lookup to use primary key index, got {plan}"
    );
}

#[test]
fn user_api_key_lookup_uses_owner_key_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", user_api_key_ids_for_user_sql()),
            ["user-1"],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("idx_api_key_owners_user_key_lookup"),
        "expected user key owner lookup index in plan, got {plan}"
    );
}

#[test]
fn app_user_lists_use_list_order_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", app_user_list_sql()),
    );

    assert!(
        plan.contains("idx_app_users_list_order"),
        "expected app user list query to use idx_app_users_list_order, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "app user list query should avoid temp sorting, got {plan}"
    );
}
#[test]
fn app_user_role_count_queries_use_role_status_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (label, sql) in [
        ("member app user count", member_app_user_count_sql()),
        ("active admin count", active_admin_count_sql()),
    ] {
        let plan = collect_query_plan(&storage, &format!("EXPLAIN QUERY PLAN {sql}"));
        assert!(
            plan.contains("idx_app_users_role_status"),
            "{label} should use role/status index, got {plan}"
        );
    }
}

#[test]
fn app_project_user_relationship_queries_use_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let owned_projects_plan = collect_query_plan(
        &storage,
        "EXPLAIN QUERY PLAN
         SELECT id
         FROM app_projects
         WHERE owner_user_id = 'user-1'",
    );
    let memberships_plan = collect_query_plan(
        &storage,
        "EXPLAIN QUERY PLAN
         SELECT project_id
         FROM app_project_members
         WHERE user_id = 'user-1'
         ORDER BY project_id ASC",
    );

    assert!(
        owned_projects_plan.contains("idx_app_projects_owner_user_lookup"),
        "expected owned project lookup to use owner user index, got {owned_projects_plan}"
    );
    assert!(
        memberships_plan.contains("idx_app_project_members_user_lookup"),
        "expected project member lookup to use user lookup index, got {memberships_plan}"
    );
    assert!(
        !memberships_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "project member lookup should avoid temp sorting, got {memberships_plan}"
    );
}

#[test]
fn account_manager_chunk_queries_defer_final_ordering_to_rust() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let dashboard_users_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            dashboard_app_user_summary_sql(Some("u.id IN ('user-a', 'user-b')"))
        ),
    );
    let access_users_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            app_user_access_summary_sql("id IN ('user-a', 'user-b')")
        ),
    );
    let api_key_owners_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            api_key_owner_chunk_sql("key_id IN ('key-a', 'key-b')")
        ),
    );
    let user_wallets_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            user_wallets_for_users_chunk_sql("owner_id IN ('user-a', 'user-b')")
        ),
    );

    assert!(
        dashboard_users_plan.contains("sqlite_autoindex_app_users"),
        "expected dashboard user chunk query to use app user id lookup index, got {dashboard_users_plan}"
    );
    assert!(
        !dashboard_users_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "dashboard user chunk query should avoid per-chunk ORDER BY temp sorting, got {dashboard_users_plan}"
    );
    assert!(
        access_users_plan.contains("sqlite_autoindex_app_users"),
        "expected access user chunk query to use app user id lookup index, got {access_users_plan}"
    );
    assert!(
        !access_users_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "access user chunk query should avoid per-chunk ORDER BY temp sorting, got {access_users_plan}"
    );
    assert!(
        api_key_owners_plan.contains("sqlite_autoindex_api_key_owners"),
        "expected API key owner chunk query to use key owner lookup index, got {api_key_owners_plan}"
    );
    assert!(
        !api_key_owners_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "API key owner chunk query should avoid per-chunk ORDER BY temp sorting, got {api_key_owners_plan}"
    );
    assert!(
        user_wallets_plan.contains("sqlite_autoindex_app_wallets"),
        "expected user wallet chunk query to use wallet owner lookup index, got {user_wallets_plan}"
    );
    assert!(
        !user_wallets_plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "user wallet chunk query should avoid per-chunk ORDER BY temp sorting, got {user_wallets_plan}"
    );
}

#[test]
fn api_key_owner_rows_return_key_ordered_rows() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO api_keys (id, name, key_hash, status, created_at, last_used_at)
             VALUES
                ('key-a', 'Key A', 'hash-a', 'enabled', 1, NULL),
                ('key-b', 'Key B', 'hash-b', 'enabled', 2, NULL)",
            [],
        )
        .expect("seed api keys");
    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES ('user-b', 'user-b@example.com', NULL, 'hash', 'member', 'active', 1, 1, NULL)",
            [],
        )
        .expect("seed user");
    storage
        .conn
        .execute(
            "INSERT INTO app_projects (id, name, owner_user_id, status, created_at, updated_at)
             VALUES ('project-a', 'Project A', NULL, 'active', 1, 1)",
            [],
        )
        .expect("seed project");
    storage
        .conn
        .execute(
            "INSERT INTO api_key_owners (
                key_id, owner_kind, owner_user_id, project_id, updated_at
             ) VALUES
                ('key-b', 'user', 'user-b', NULL, 2),
                ('key-a', 'project', NULL, 'project-a', 1)",
            [],
        )
        .expect("seed owners");

    let owners = storage.list_api_key_owner_rows().expect("read owner rows");

    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0].key_id, "key-a");
    assert_eq!(owners[0].owner_kind, "project");
    assert_eq!(owners[0].project_id.as_deref(), Some("project-a"));
    assert_eq!(owners[1].key_id, "key-b");
    assert_eq!(owners[1].owner_kind, "user");
    assert_eq!(owners[1].owner_user_id.as_deref(), Some("user-b"));

    let plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", api_key_owner_rows_sql()),
    );
    assert!(
        plan.contains("sqlite_autoindex_api_key_owners"),
        "expected ordered API key owner rows to scan key owner primary key, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "ordered API key owner rows should avoid temp sorting, got {plan}"
    );
}

#[test]
fn request_charge_count_uses_entry_kind_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!(
                "EXPLAIN QUERY PLAN {}",
                request_charge_ledger_entry_count_sql()
            ),
            [],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("idx_app_wallet_ledger_entry_kind"),
        "expected wallet ledger entry kind index in plan, got {plan}"
    );
}

#[test]
fn user_wallet_available_credit_filters_to_user_wallets() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-1', 'user', 'user-1', 1000, 250, 'active', 1, 1),
                ('wallet-project-1', 'project', 'project-1', 5000, 0, 'active', 1, 1)",
            [],
        )
        .expect("seed wallets");

    let balances = storage
        .user_wallet_available_credit_micros()
        .expect("read user wallet balances");

    assert_eq!(balances, vec![("user-1".to_string(), 750)]);
}

#[test]
fn dashboard_app_user_summaries_join_user_wallets() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 1, NULL),
                ('user-2', 'admin@example.com', NULL, 'admin-hash', 'admin', 'disabled', 2, 2, NULL)",
            [],
        )
        .expect("seed app users");
    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-1', 'user', 'user-1', 1000, 250, 'active', 1, 1),
                ('wallet-project-1', 'project', 'user-2', 5000, 0, 'active', 1, 1)",
            [],
        )
        .expect("seed wallets");

    let users = storage
        .list_dashboard_app_user_summaries()
        .expect("read dashboard users");

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].id, "user-1");
    assert_eq!(users[0].username, "member@example.com");
    assert_eq!(users[0].display_name.as_deref(), Some("Member"));
    assert_eq!(users[0].role, "member");
    assert_eq!(users[0].status, "active");
    assert_eq!(users[0].wallet_available_credit_micros, Some(750));
    assert_eq!(users[1].id, "user-2");
    assert_eq!(users[1].wallet_available_credit_micros, None);
}

#[test]
fn dashboard_app_user_summaries_for_ids_filters_requested_users() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 1, NULL),
                ('user-2', 'admin@example.com', NULL, 'admin-hash', 'admin', 'disabled', 2, 2, NULL),
                ('user-3', 'unused@example.com', NULL, 'unused-hash', 'member', 'active', 3, 3, NULL)",
            [],
        )
        .expect("seed app users");
    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-2', 'user', 'user-2', 2000, 500, 'active', 1, 1)",
            [],
        )
        .expect("seed wallets");

    let users = storage
        .list_dashboard_app_user_summaries_for_ids(&[
            "user-2".to_string(),
            "missing".to_string(),
            "user-1".to_string(),
            "user-2".to_string(),
        ])
        .expect("read dashboard users by ids");

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].id, "user-1");
    assert_eq!(users[0].wallet_available_credit_micros, None);
    assert_eq!(users[1].id, "user-2");
    assert_eq!(users[1].wallet_available_credit_micros, Some(1500));
    assert!(!users.iter().any(|user| user.id == "user-3"));
}

#[test]
fn app_user_access_summaries_for_ids_project_access_fields_only() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 1, NULL),
                ('user-2', 'admin@example.com', NULL, 'admin-hash', 'admin', 'disabled', 2, 2, NULL),
                ('user-3', 'unused@example.com', NULL, 'unused-hash', 'member', 'active', 3, 3, NULL)",
            [],
        )
        .expect("seed app users");

    let users = storage
        .list_app_user_access_summaries_for_ids(&[
            "user-2".to_string(),
            "missing".to_string(),
            "user-1".to_string(),
            "user-2".to_string(),
            " ".to_string(),
        ])
        .expect("read access users by ids");

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].id, "user-1");
    assert_eq!(users[0].username, "member@example.com");
    assert_eq!(users[0].role, "member");
    assert_eq!(users[0].status, "active");
    assert_eq!(users[1].id, "user-2");
    assert_eq!(users[1].username, "admin@example.com");
    assert_eq!(users[1].role, "admin");
    assert_eq!(users[1].status, "disabled");
    assert!(!users.iter().any(|user| user.id == "user-3"));
}

#[test]
fn api_key_owners_for_ids_filters_requested_keys() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for key_id in ["key-1", "key-2", "key-unused"] {
        seed_api_key(&storage, key_id);
    }
    seed_app_user(&storage, "user-1");
    seed_app_user(&storage, "user-unused");
    seed_app_user(&storage, "project-owner");
    seed_app_project(&storage, "project-1", "project-owner");

    for owner in [
        ApiKeyOwner {
            key_id: "key-1".to_string(),
            owner_kind: "user".to_string(),
            owner_user_id: Some("user-1".to_string()),
            project_id: None,
            updated_at: 1,
        },
        ApiKeyOwner {
            key_id: "key-2".to_string(),
            owner_kind: "project".to_string(),
            owner_user_id: None,
            project_id: Some("project-1".to_string()),
            updated_at: 2,
        },
        ApiKeyOwner {
            key_id: "key-unused".to_string(),
            owner_kind: "user".to_string(),
            owner_user_id: Some("user-unused".to_string()),
            project_id: None,
            updated_at: 3,
        },
    ] {
        storage
            .upsert_api_key_owner(&owner)
            .expect("seed api key owner");
    }

    let owners = storage
        .list_api_key_owners_for_ids(&[
            " key-2 ".to_string(),
            "missing".to_string(),
            "key-1".to_string(),
            "key-2".to_string(),
            " ".to_string(),
        ])
        .expect("read owners by ids");

    assert_eq!(
        owners
            .iter()
            .map(|owner| owner.key_id.as_str())
            .collect::<Vec<_>>(),
        vec!["key-1", "key-2"]
    );
    assert_eq!(owners[0].owner_user_id.as_deref(), Some("user-1"));
    assert_eq!(owners[1].project_id.as_deref(), Some("project-1"));
    assert!(!owners.iter().any(|owner| owner.key_id == "key-unused"));
}

#[test]
fn api_key_owners_for_ids_chunks_large_key_sets() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for index in 0..950 {
        let key_id = format!("key-{index:04}");
        let user_id = format!("user-{index:04}");
        seed_api_key(&storage, &key_id);
        seed_app_user(&storage, &user_id);
        let owner = ApiKeyOwner {
            key_id,
            owner_kind: "user".to_string(),
            owner_user_id: Some(user_id),
            project_id: None,
            updated_at: i64::from(index),
        };
        storage
            .upsert_api_key_owner(&owner)
            .expect("seed api key owner");
    }

    let key_ids = (0..950)
        .map(|index| format!("key-{index:04}"))
        .collect::<Vec<_>>();
    let owners = storage
        .list_api_key_owners_for_ids(&key_ids)
        .expect("read chunked owners");

    assert_eq!(owners.len(), 950);
    assert!(owners.iter().any(|owner| owner.key_id == "key-0949"));
}

#[test]
fn app_user_access_summary_by_id_projects_access_fields_only() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 1, NULL)",
            [],
        )
        .expect("seed app user");

    let user = storage
        .find_app_user_access_summary_by_id("user-1")
        .expect("read access user")
        .expect("user exists");
    assert_eq!(user.id, "user-1");
    assert_eq!(user.username, "member@example.com");
    assert_eq!(user.role, "member");
    assert_eq!(user.status, "active");

    assert!(storage
        .find_app_user_access_summary_by_id("missing")
        .expect("read missing access user")
        .is_none());

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", app_user_access_summary_by_id_sql()),
            ["user-1"],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");
    assert!(
        plan.contains("sqlite_autoindex_app_users"),
        "expected app user access summary lookup to use primary key index, got {plan}"
    );
}

#[test]
fn public_app_users_with_wallets_project_public_fields_without_password_hash() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 2, 3),
                ('user-2', 'admin@example.com', NULL, 'admin-hash', 'admin', 'active', 4, 5, NULL)",
            [],
        )
        .expect("seed app users");
    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-1', 'user', 'user-1', 2000, 125, 'active', 6, 7),
                ('wallet-admin-1', 'user', 'user-2', 9000, 0, 'active', 8, 9),
                ('wallet-project-1', 'project', 'user-1', 5000, 0, 'active', 10, 11)",
            [],
        )
        .expect("seed wallets");

    let users = storage
        .list_public_app_users_with_wallets()
        .expect("read public users");

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].id, "user-1");
    assert_eq!(users[0].username, "member@example.com");
    assert_eq!(users[0].created_at, 1);
    assert_eq!(users[0].updated_at, 2);
    assert_eq!(users[0].last_login_at, Some(3));
    assert_eq!(users[0].wallet_id.as_deref(), Some("wallet-user-1"));
    assert_eq!(users[0].wallet_owner_kind.as_deref(), Some("user"));
    assert_eq!(users[0].wallet_balance_credit_micros, Some(2000));
    assert_eq!(users[0].wallet_frozen_credit_micros, Some(125));
    assert_eq!(users[1].id, "user-2");
    assert_eq!(users[1].wallet_id, None);

    let plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            public_app_user_with_wallet_sql(None, true)
        ),
    );
    assert!(
        plan.contains("idx_app_users_list_order"),
        "expected public app user list to use app user list-order index, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "public app user list should avoid temp sorting, got {plan}"
    );
}

#[test]
fn public_app_user_with_wallet_by_id_filters_single_public_user() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-1', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 2, 3),
                ('user-2', 'admin@example.com', NULL, 'admin-hash', 'admin', 'active', 4, 5, NULL)",
            [],
        )
        .expect("seed app users");
    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-1', 'user', 'user-1', 2000, 125, 'active', 6, 7),
                ('wallet-admin-1', 'user', 'user-2', 9000, 0, 'active', 8, 9)",
            [],
        )
        .expect("seed wallets");

    let member = storage
        .find_public_app_user_with_wallet_by_id("user-1")
        .expect("read member public user")
        .expect("member exists");
    assert_eq!(member.username, "member@example.com");
    assert_eq!(member.wallet_id.as_deref(), Some("wallet-user-1"));
    assert_eq!(member.wallet_balance_credit_micros, Some(2000));
    assert_eq!(member.wallet_frozen_credit_micros, Some(125));

    let admin = storage
        .find_public_app_user_with_wallet_by_id("user-2")
        .expect("read admin public user")
        .expect("admin exists");
    assert_eq!(admin.username, "admin@example.com");
    assert_eq!(admin.wallet_id, None);

    assert!(storage
        .find_public_app_user_with_wallet_by_id("missing")
        .expect("read missing public user")
        .is_none());

    let sql = format!(
        "{}\n         LIMIT 1",
        public_app_user_with_wallet_sql(Some("u.id = 'user-1'"), false)
    );
    let plan = collect_query_plan(&storage, &format!("EXPLAIN QUERY PLAN {sql}"));
    assert!(
        plan.contains("sqlite_autoindex_app_users"),
        "expected public app user lookup to use app user primary-key index, got {plan}"
    );
}

#[test]
fn active_app_session_user_by_token_hash_joins_public_user_and_wallet() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_users (
                id, username, display_name, password_hash, role, status,
                created_at, updated_at, last_login_at
             ) VALUES
                ('user-active', 'member@example.com', 'Member', 'secret-hash', 'member', 'active', 1, 2, 3),
                ('user-admin', 'admin@example.com', NULL, 'admin-hash', 'admin', 'active', 4, 5, NULL),
                ('user-disabled', 'disabled@example.com', NULL, 'disabled-hash', 'member', 'disabled', 6, 7, NULL)",
            [],
        )
        .expect("seed app users");
    storage
        .conn
        .execute(
            "INSERT INTO app_user_sessions (
                id, user_id, token_hash, expires_at, created_at, last_seen_at, revoked_at
             ) VALUES
                ('session-active', 'user-active', 'hash-active', 100, 1, NULL, NULL),
                ('session-admin', 'user-admin', 'hash-admin', 100, 1, NULL, NULL),
                ('session-disabled', 'user-disabled', 'hash-disabled', 100, 1, NULL, NULL),
                ('session-expired', 'user-active', 'hash-expired', 10, 1, NULL, NULL),
                ('session-revoked', 'user-active', 'hash-revoked', 100, 1, NULL, 2)",
            [],
        )
        .expect("seed sessions");
    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-active', 'user', 'user-active', 2000, 125, 'active', 8, 9),
                ('wallet-admin', 'user', 'user-admin', 9000, 0, 'active', 10, 11),
                ('wallet-project', 'project', 'user-active', 5000, 0, 'active', 12, 13)",
            [],
        )
        .expect("seed wallets");

    let active = storage
        .find_active_app_session_user_by_token_hash("hash-active", 50)
        .expect("find active session")
        .expect("active session exists");

    assert_eq!(active.session_id, "session-active");
    assert_eq!(active.expires_at, 100);
    assert_eq!(active.user.id, "user-active");
    assert_eq!(active.user.username, "member@example.com");
    assert_eq!(active.user.wallet_id.as_deref(), Some("wallet-active"));
    assert_eq!(active.user.wallet_balance_credit_micros, Some(2000));
    assert_eq!(active.user.wallet_frozen_credit_micros, Some(125));

    let admin = storage
        .find_active_app_session_user_by_token_hash("hash-admin", 50)
        .expect("find admin session")
        .expect("admin session exists");
    assert_eq!(admin.user.id, "user-admin");
    assert_eq!(admin.user.wallet_id, None);

    for token_hash in [
        "hash-disabled",
        "hash-expired",
        "hash-revoked",
        "hash-missing",
    ] {
        assert!(storage
            .find_active_app_session_user_by_token_hash(token_hash, 50)
            .expect("find inactive session")
            .is_none());
    }

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            active_app_session_user_with_wallet_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query(("hash-active", 50_i64)).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }
    assert!(
        plan.contains("idx_app_user_sessions_token_hash")
            || plan.contains("sqlite_autoindex_app_user_sessions_2"),
        "expected active session user lookup to use token hash index, got {plan}"
    );
    assert!(
        plan.contains("sqlite_autoindex_app_users"),
        "expected active session user lookup to join app users by primary key, got {plan}"
    );
}

#[test]
fn user_wallets_for_users_filters_requested_user_wallets() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .conn
        .execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES
                ('wallet-user-1', 'user', 'user-1', 1000, 100, 'active', 1, 1),
                ('wallet-user-2', 'user', 'user-2', 2000, 0, 'active', 1, 1),
                ('wallet-project-1', 'project', 'user-1', 5000, 0, 'active', 1, 1)",
            [],
        )
        .expect("seed wallets");

    let wallets = storage
        .user_wallets_for_users(&[
            "user-1".to_string(),
            "user-1".to_string(),
            "missing-user".to_string(),
            " ".to_string(),
        ])
        .expect("read user wallets");

    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0].owner_id, "user-1");
    assert_eq!(wallets[0].owner_kind, "user");
    assert_eq!(wallets[0].balance_credit_micros, 1000);
}

#[test]
fn user_wallets_for_users_chunks_large_user_sets() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let user_ids = (0..950)
        .map(|idx| format!("user-{idx}"))
        .collect::<Vec<_>>();
    let tx = storage.conn.unchecked_transaction().expect("begin tx");
    for (idx, user_id) in user_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO app_wallets (
                id, owner_kind, owner_id, balance_credit_micros, frozen_credit_micros,
                status, created_at, updated_at
             ) VALUES (?1, 'user', ?2, ?3, 0, 'active', 1, 1)",
            (
                format!("wallet-{idx}"),
                user_id,
                i64::try_from(idx).expect("idx fits i64"),
            ),
        )
        .expect("seed wallet");
    }
    tx.commit().expect("commit wallets");

    let wallets = storage
        .user_wallets_for_users(&user_ids)
        .expect("read chunked wallets");

    assert_eq!(wallets.len(), 950);
    assert!(wallets.iter().any(|wallet| wallet.owner_id == "user-949"));
}

#[test]
fn user_wallet_available_credit_uses_owner_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", user_wallet_available_credit_sql()),
            [],
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("sqlite_autoindex_app_wallets_2"),
        "expected wallet unique owner index in plan, got {plan}"
    );
}

#[test]
fn wallet_owner_lookup_uses_unique_owner_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = storage
        .conn
        .query_row(
            &format!("EXPLAIN QUERY PLAN {}", app_wallet_by_owner_sql()),
            ("user", "user-1"),
            |row| row.get::<_, String>(3),
        )
        .expect("explain plan");

    assert!(
        plan.contains("sqlite_autoindex_app_wallets_2"),
        "expected wallet owner lookup to use unique owner index, got {plan}"
    );
}
