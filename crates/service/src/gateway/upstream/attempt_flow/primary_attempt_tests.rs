use super::should_failover_transport_error;

#[test]
fn chatgpt_transport_error_fails_over_when_more_candidates_exist() {
    assert!(should_failover_transport_error(
        "https://chatgpt.com/backend-api/codex/responses",
        true
    ));
}

#[test]
fn chatgpt_transport_error_stops_without_more_candidates() {
    assert!(!should_failover_transport_error(
        "https://chatgpt.com/backend-api/codex/responses",
        false
    ));
}

#[test]
fn openai_api_transport_error_keeps_terminal_behavior() {
    assert!(!should_failover_transport_error(
        "https://api.openai.com/v1/responses",
        true
    ));
}

#[test]
fn custom_upstream_transport_error_keeps_existing_failover_behavior() {
    assert!(should_failover_transport_error(
        "https://example.test/v1/responses",
        true
    ));
}
