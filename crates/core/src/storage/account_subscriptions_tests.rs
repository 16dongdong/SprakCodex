use super::*;
use crate::storage::{now_ts, Account};

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
fn list_account_subscriptions_for_accounts_filters_to_requested_ids() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();

    for (account_id, plan) in [("acc-a", "free"), ("acc-b", "plus")] {
        storage
            .insert_account(&sample_account(account_id, now))
            .expect("insert account");
        storage
            .upsert_account_subscription(account_id, true, Some(plan), Some(plan), None, None)
            .expect("upsert subscription");
    }

    let requested = vec!["acc-b".to_string(), "missing".to_string()];
    let items = storage
        .list_account_subscriptions_for_accounts(&requested)
        .expect("list subscriptions");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].account_id, "acc-b");
    assert_eq!(items[0].account_plan_type.as_deref(), Some("plus"));
}

#[test]
fn list_account_subscriptions_for_accounts_chunks_large_account_sets() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();

    let target = "acc-0949";
    storage
        .insert_account(&sample_account(target, now))
        .expect("insert target account");
    storage
        .upsert_account_subscription(target, true, Some("team"), Some("team"), None, None)
        .expect("upsert subscription");

    let requested = (0..950)
        .map(|index| format!("acc-{index:04}"))
        .collect::<Vec<_>>();
    let items = storage
        .list_account_subscriptions_for_accounts(&requested)
        .expect("list subscriptions");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].account_id, target);
    assert_eq!(items[0].account_plan_type.as_deref(), Some("team"));
}

#[test]
fn account_subscription_chunk_query_defers_final_ordering_to_rust() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let sql = account_subscriptions_for_accounts_chunk_sql("account_id IN ('acc-a', 'acc-b')");
    let mut stmt = storage
        .conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("sqlite_autoindex_account_subscriptions_1") || plan.contains("USING INDEX"),
        "subscription chunk query should use account_id lookup index, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "subscription chunk query should avoid per-chunk ORDER BY temp sorting, got {plan}"
    );
}

#[test]
fn account_subscription_lookup_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            account_subscription_by_account_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query(["acc-a"]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("sqlite_autoindex_account_subscriptions_1") || plan.contains("USING INDEX"),
        "subscription lookup should use account_id primary-key index, got {plan}"
    );
}

#[test]
fn account_subscription_list_uses_updated_at_order_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            account_subscription_list_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_account_subscriptions_updated_at"),
        "subscription list should use updated-at order index, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "subscription list should avoid temp ORDER BY sorting, got {plan}"
    );
}
#[test]
fn account_subscription_delete_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            delete_account_subscription_for_account_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query(["acc-a"]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("sqlite_autoindex_account_subscriptions_1") || plan.contains("USING INDEX"),
        "subscription delete should use account_id primary-key index, got {plan}"
    );
}
