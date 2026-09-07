use super::*;

#[test]
fn sseUsageUsesServerTotal() {
    let mut parser = UsageParser::default();
    parser.feed(b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-test\",\"usage\":{\"input_tokens\":100,\"input_tokens_details\":{\"cached_tokens\":40},\"output_tokens\":20,\"total_tokens\":120}}}\n\n", true);
    parser.finish(true);
    assert_eq!(parser.usage.total_tokens, Some(120));
    assert_eq!(parser.usage.cached_input_tokens, Some(40));
    assert!(parser.terminal);
    assert_eq!(parser.responseId.as_deref(), Some("resp_test"));
}

#[test]
fn incompleteSseIsUnknown() {
    let mut parser = UsageParser::default();
    parser.feed(b"data: {\"type\":\"response.output_text.delta\"}\n", true);
    parser.finish(true);
    assert!(parser.usage.total_tokens.is_none());
    assert!(parser.problem.is_some());
}

#[test]
fn eventLimitDoesNotStoreBody() {
    let mut parser = UsageParser::default();
    parser.feed(&vec![b'x'; maxEventBytes + 1], false);
    parser.finish(false);
    assert!(parser.problem.is_some());
    assert!(parser.event.is_empty());
}
