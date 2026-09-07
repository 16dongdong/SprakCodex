use super::*;

#[test]
fn extract_usage_limit_ignores_plain_text_delta() {
    let lines = vec![
        "event: response.output_text.delta\n".to_string(),
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"You've hit your usage limit. To get more access now, send a request to your admin or try again at 7:44 PM.\"}\n".to_string(),
    ];
    assert!(extract_usage_limit_from_sse_data(&lines).is_none());
}

#[test]
fn extract_usage_limit_matches_quota_exceeded_json() {
    let lines = vec![
        "data: {\"error\":{\"code\":\"insufficient_quota\",\"message\":\"quota exceeded\"}}\n"
            .to_string(),
    ];
    assert!(extract_usage_limit_from_sse_data(&lines).is_some());
}

#[test]
fn extract_usage_limit_ignores_unrelated_content() {
    let lines = vec![
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello world\"}\n".to_string(),
    ];
    assert!(extract_usage_limit_from_sse_data(&lines).is_none());
}

#[test]
fn extract_usage_limit_ignores_frames_without_data() {
    let lines = vec!["event: ping\n".to_string(), ": keepalive\n".to_string()];
    assert!(extract_usage_limit_from_sse_data(&lines).is_none());
}
