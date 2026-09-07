use super::{
    read_requestlog_today_summary_for_key_ids_with_storage, resolve_day_bounds_ts,
    MAX_REQUESTED_DAY_RANGE_SECS,
};
use codexmanager_core::storage::Storage;

#[test]
fn resolve_day_bounds_uses_requested_range_when_complete() {
    assert_eq!(
        resolve_day_bounds_ts(Some(1_700_000_000), Some(1_700_086_400)).unwrap(),
        (1_700_000_000, 1_700_086_400)
    );
}

#[test]
fn resolve_day_bounds_rejects_partial_range() {
    let error = resolve_day_bounds_ts(Some(1_700_000_000), None).unwrap_err();
    assert!(error.contains("provided together"));
}

#[test]
fn resolve_day_bounds_rejects_oversized_range() {
    let error = resolve_day_bounds_ts(Some(0), Some(MAX_REQUESTED_DAY_RANGE_SECS + 1)).unwrap_err();
    assert!(error.contains("too large"));
}

#[test]
fn today_summary_short_circuits_empty_key_ids() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let summary = read_requestlog_today_summary_for_key_ids_with_storage(
        &storage,
        Some(1_700_000_000),
        Some(1_700_086_400),
        &[],
    )
    .expect("read empty member today summary");

    assert_eq!(summary.input_tokens, 0);
    assert_eq!(summary.cached_input_tokens, 0);
    assert_eq!(summary.output_tokens, 0);
    assert_eq!(summary.reasoning_output_tokens, 0);
    assert_eq!(summary.today_tokens, 0);
    assert_eq!(summary.estimated_cost, 0.0);
}
