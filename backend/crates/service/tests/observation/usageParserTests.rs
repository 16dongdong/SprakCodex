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

// 完整正文保留心跳和大响应，独立用量解析仍识别末尾终态。
#[test]
fn completeStreamKeepsBodyAndUsage() {
    let mut parser = UsageParser::default();
    parser.feed(b": heartbeat\n\n", true);
    let event = serde_json::json!({"type":"response.completed","response":{"output":"x".repeat(512*1024),"usage":{"input_tokens":12,"output_tokens":1}}});
    let body = format!("data: {event}\n\n");
    parser.feed(body.as_bytes(), true);
    parser.finish(true);
    assert_eq!(parser.usage.total_tokens, Some(13));
    assert!(parser.body.snapshot().as_str().unwrap().len() > 512 * 1024);
}
