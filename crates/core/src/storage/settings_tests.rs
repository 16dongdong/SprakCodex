use super::*;

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
fn app_setting_lookup_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = app_setting_value_by_key_sql().replace("?1", "'gateway.routeStrategy'");
    let plan = collect_query_plan(&storage, &format!("EXPLAIN QUERY PLAN {sql}"));

    assert!(
        plan.contains("sqlite_autoindex_app_settings_1"),
        "expected app setting lookup to use primary-key index, got {plan}"
    );
}

#[test]
fn app_settings_list_uses_primary_key_order_without_temp_sort() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let plan = collect_query_plan(
        &storage,
        &format!("EXPLAIN QUERY PLAN {}", app_settings_list_sql()),
    );

    assert!(
        plan.contains("sqlite_autoindex_app_settings_1"),
        "expected app settings list to scan primary-key order, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "expected app settings list to avoid temp ordering, got {plan}"
    );
}
