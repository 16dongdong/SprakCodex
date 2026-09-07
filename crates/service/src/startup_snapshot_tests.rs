use super::normalize_startup_request_log_limit;

#[test]
fn startup_request_log_limit_defaults_to_light_sample() {
    assert_eq!(normalize_startup_request_log_limit(None), 24);
}

#[test]
fn startup_request_log_limit_clamps_large_values() {
    assert_eq!(normalize_startup_request_log_limit(Some(120)), 24);
    assert_eq!(normalize_startup_request_log_limit(Some(500)), 24);
}

#[test]
fn startup_request_log_limit_keeps_smaller_values_and_allows_zero() {
    assert_eq!(normalize_startup_request_log_limit(Some(8)), 8);
    assert_eq!(normalize_startup_request_log_limit(Some(0)), 0);
    assert_eq!(normalize_startup_request_log_limit(Some(-1)), 0);
}
