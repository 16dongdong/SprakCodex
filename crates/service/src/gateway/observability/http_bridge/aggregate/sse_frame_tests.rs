use super::{
    inspect_openai_responses_sse_frame, inspect_sse_frame, inspect_sse_frame_for_protocol,
    PassthroughSseProtocol, SseTerminal,
};

/// 函数 `inspect_sse_frame_keeps_last_event_type_from_header`
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
fn inspect_sse_frame_keeps_last_event_type_from_header() {
    let lines = vec![
        "event: response.completed\n".to_string(),
        "data: {\"type\":\"response.completed\"}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection = inspect_sse_frame(&lines);
    assert_eq!(
        inspection.last_event_type.as_deref(),
        Some("response.completed")
    );
}

/// 函数 `inspect_sse_frame_keeps_last_event_type_from_json_type`
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
fn inspect_sse_frame_keeps_last_event_type_from_json_type() {
    let lines = vec![
        "data: {\"type\":\"response.failed\",\"error\":{\"message\":\"oops\"}}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection = inspect_sse_frame(&lines);
    assert_eq!(
        inspection.last_event_type.as_deref(),
        Some("response.failed")
    );
}

#[test]
fn inspect_sse_frame_generic_mode_does_not_treat_message_stop_as_terminal() {
    let lines = vec![
        "event: message_stop\n".to_string(),
        "data: {\"type\":\"message_stop\"}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection = inspect_sse_frame_for_protocol(&lines, PassthroughSseProtocol::Generic);
    assert!(inspection.terminal.is_none());
    assert_eq!(inspection.last_event_type.as_deref(), Some("message_stop"));
}

#[test]
fn inspect_sse_frame_anthropic_native_treats_message_stop_as_terminal() {
    let lines = vec![
        "event: message_stop\n".to_string(),
        "data: {\"type\":\"message_stop\"}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection =
        inspect_sse_frame_for_protocol(&lines, PassthroughSseProtocol::AnthropicNative);
    assert!(matches!(inspection.terminal, Some(SseTerminal::Ok)));
    assert_eq!(inspection.last_event_type.as_deref(), Some("message_stop"));
}

#[test]
fn inspect_openai_responses_sse_frame_collects_output_item_text() {
    let lines = vec![
        "event: response.output_item.done\n".to_string(),
        "data: {\"output_item\":{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello from output_item\"}]}}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection = inspect_openai_responses_sse_frame(&lines);
    let usage = inspection.usage.expect("usage");
    assert_eq!(usage.output_text.as_deref(), Some("hello from output_item"));
    assert_eq!(
        inspection.last_event_type.as_deref(),
        Some("response.output_item.done")
    );
}

#[test]
fn inspect_openai_responses_sse_frame_collects_structured_delta_text() {
    let lines = vec![
        "event: response.output_text.delta\n".to_string(),
        "data: {\"delta\":{\"text\":\"hello from structured delta\"}}\n".to_string(),
        "\n".to_string(),
    ];
    let inspection = inspect_openai_responses_sse_frame(&lines);
    let usage = inspection.usage.expect("usage");
    assert_eq!(
        usage.output_text.as_deref(),
        Some("hello from structured delta")
    );
    assert_eq!(
        inspection.last_event_type.as_deref(),
        Some("response.output_text.delta")
    );
}
