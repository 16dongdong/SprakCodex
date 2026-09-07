use super::*;

#[test]
fn request_log_filter_builder_marks_token_stats_usage_only_when_needed() {
    let exact_filters = build_request_log_filters(
        Some("model:=gpt-5"),
        Some("2xx"),
        Some(1000),
        Some(2000),
        false,
        None,
        true,
    );
    assert!(!exact_filters.uses_token_stats);
    assert!(!exact_filters.uses_account_lookup);

    let global_filters = build_request_log_filters(Some("42"), None, None, None, true, None, true);
    assert!(global_filters.uses_token_stats);
    assert!(global_filters.uses_account_lookup);

    let account_filters =
        build_request_log_filters(Some("account:team"), None, None, None, true, None, true);
    assert!(!account_filters.uses_token_stats);
    assert!(account_filters.uses_account_lookup);

    let account_without_table_filters =
        build_request_log_filters(Some("account:team"), None, None, None, false, None, true);
    assert!(!account_without_table_filters.uses_token_stats);
    assert!(!account_without_table_filters.uses_account_lookup);
}
