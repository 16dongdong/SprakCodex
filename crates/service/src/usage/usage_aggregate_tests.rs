use super::{compute_usage_aggregate_summary, compute_usage_aggregate_summary_for_known_items};
use codexmanager_core::storage::{now_ts, Account, UsageSnapshotRecord};

/// 函数 `account`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
fn account(id: &str) -> Account {
    Account {
        id: id.to_string(),
        label: id.to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: now_ts(),
        updated_at: now_ts(),
    }
}

/// 函数 `usage_record`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - account_id: 参数 account_id
/// - used_percent: 参数 used_percent
/// - window_minutes: 参数 window_minutes
/// - secondary_used_percent: 参数 secondary_used_percent
/// - secondary_window_minutes: 参数 secondary_window_minutes
/// - credits_json: 参数 credits_json
///
/// # 返回
/// 返回函数执行结果
fn usage_record(
    account_id: &str,
    used_percent: Option<f64>,
    window_minutes: Option<i64>,
    secondary_used_percent: Option<f64>,
    secondary_window_minutes: Option<i64>,
    credits_json: Option<&str>,
) -> UsageSnapshotRecord {
    UsageSnapshotRecord {
        account_id: account_id.to_string(),
        used_percent,
        window_minutes,
        resets_at: None,
        secondary_used_percent,
        secondary_window_minutes,
        secondary_resets_at: None,
        credits_json: credits_json.map(|value| value.to_string()),
        captured_at: now_ts(),
    }
}

/// 函数 `aggregate_summary_routes_free_single_window_account_to_secondary_bucket`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn aggregate_summary_routes_free_single_window_account_to_secondary_bucket() {
    let accounts = vec![account("a1"), account("a2")];
    let usage_items = vec![
        usage_record("a1", Some(20.0), Some(300), Some(40.0), Some(10080), None),
        usage_record(
            "a2",
            Some(10.0),
            Some(10080),
            None,
            None,
            Some(r#"{"planType":"free"}"#),
        ),
    ];

    let result = compute_usage_aggregate_summary(&accounts, &usage_items);
    assert_eq!(result.primary_bucket_count, 1);
    assert_eq!(result.primary_known_count, 1);
    assert_eq!(result.primary_remain_percent, Some(80));
    assert_eq!(result.secondary_bucket_count, 2);
    assert_eq!(result.secondary_known_count, 2);
    assert_eq!(result.secondary_remain_percent, Some(75));
}

/// 函数 `aggregate_summary_preserves_unknown_counts_per_bucket`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn aggregate_summary_preserves_unknown_counts_per_bucket() {
    let accounts = vec![account("a1"), account("a2"), account("a3")];
    let usage_items = vec![
        usage_record("a1", Some(20.0), Some(300), None, None, None),
        usage_record(
            "a2",
            None,
            Some(10080),
            None,
            None,
            Some(r#"{"planType":"free"}"#),
        ),
    ];

    let result = compute_usage_aggregate_summary(&accounts, &usage_items);
    assert_eq!(result.primary_bucket_count, 1);
    assert_eq!(result.primary_known_count, 1);
    assert_eq!(result.primary_unknown_count, 0);
    assert_eq!(result.secondary_bucket_count, 1);
    assert_eq!(result.secondary_known_count, 0);
    assert_eq!(result.secondary_unknown_count, 1);
    assert_eq!(result.secondary_remain_percent, None);
}

#[test]
fn known_item_aggregate_skips_blank_account_ids_without_hashing_by_account() {
    let usage_items = vec![
        usage_record("account-a", Some(10.0), Some(300), None, None, None),
        usage_record(" ", Some(40.0), Some(300), None, None, None),
    ];

    let result = compute_usage_aggregate_summary_for_known_items(&usage_items);

    assert_eq!(result.primary_bucket_count, 1);
    assert_eq!(result.primary_known_count, 1);
    assert_eq!(result.primary_remain_percent, Some(90));
}
