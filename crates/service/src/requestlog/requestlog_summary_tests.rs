use super::{
    map_filter_summary, needs_unfiltered_total_count,
    read_request_log_filter_summary_for_key_ids_with_storage,
};
use codexmanager_core::rpc::types::RequestLogListParams;
use codexmanager_core::storage::{RequestLogQuerySummary, Storage};

#[test]
fn member_filter_summary_short_circuits_empty_key_ids() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let summary = read_request_log_filter_summary_for_key_ids_with_storage(
        &storage,
        RequestLogListParams {
            query: Some("trace:=ignored".to_string()),
            status_filter: Some("5xx".to_string()),
            start_ts: Some(1_700_000_000),
            end_ts: Some(1_700_086_400),
            ..RequestLogListParams::default()
        },
        &[],
    )
    .expect("read empty member request log summary");

    assert_eq!(summary.total_count, 0);
    assert_eq!(summary.filtered_count, 0);
    assert_eq!(summary.success_count, 0);
    assert_eq!(summary.error_count, 0);
    assert_eq!(summary.total_tokens, 0);
    assert_eq!(summary.total_cost_usd, 0.0);
}

#[test]
fn filter_summary_mapping_clamps_negative_aggregate_values() {
    let summary = map_filter_summary(
        -1,
        RequestLogQuerySummary {
            count: -2,
            success_count: -3,
            error_count: -4,
            total_tokens: -5,
            estimated_cost_usd: -0.25,
        },
    );

    assert_eq!(summary.total_count, 0);
    assert_eq!(summary.filtered_count, 0);
    assert_eq!(summary.success_count, 0);
    assert_eq!(summary.error_count, 0);
    assert_eq!(summary.total_tokens, 0);
    assert_eq!(summary.total_cost_usd, 0.0);
}

#[test]
fn filter_summary_reuses_filtered_count_when_status_filter_is_absent() {
    assert!(!needs_unfiltered_total_count(None));
    assert!(needs_unfiltered_total_count(Some("2xx")));
    assert!(needs_unfiltered_total_count(Some("4xx")));
    assert!(needs_unfiltered_total_count(Some("5xx")));
}
