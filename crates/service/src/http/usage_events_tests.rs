use super::*;

#[test]
fn usage_refresh_sse_frame_uses_named_event() {
    let event = crate::UsageRefreshCompletedEvent {
        source: "single",
        processed: 1,
        total: 1,
        completed_at: 1775900000,
    };

    let frame = String::from_utf8(usage_refresh_sse_frame(&event)).expect("utf8 frame");

    assert!(frame.starts_with("event: usage-refresh-completed\n"));
    assert!(frame.contains("\"source\":\"single\""));
    assert!(frame.ends_with("\n\n"));
}
