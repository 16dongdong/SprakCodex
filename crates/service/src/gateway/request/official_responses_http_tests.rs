use super::{
    apply_codex_http_request_rules, normalize_official_responses_http_body,
    normalize_official_responses_http_body_with_value,
};
use serde_json::{json, Value};

#[test]
fn responses_http_normalizer_preserves_official_shape_and_unknown_fields() {
    let body = serde_json::to_vec(&json!({
        "model": "gpt-5.4",
        "instructions": "test",
        "input": [{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}],
        "tools": [],
        "tool_choice": "auto",
        "parallel_tool_calls": false,
        "store": true,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": "thread-1",
        "client_metadata": {"k":"v"},
        "custom_passthrough": true
    }))
    .expect("serialize body");

    let normalized = normalize_official_responses_http_body("/v1/responses", body);
    let value: serde_json::Value =
        serde_json::from_slice(&normalized).expect("parse normalized body");

    assert_eq!(value["model"], "gpt-5.4");
    assert_eq!(value["tool_choice"], "auto");
    assert_eq!(value["stream"], true);
    assert_eq!(value["custom_passthrough"], true);
}

#[test]
fn responses_http_normalizer_with_value_matches_byte_normalizer() {
    let value = json!({
        "model": "gpt-5.4",
        "input": [{"role": "user", "content": [{"type": "input_text", "text": "hello"}]}],
        "custom_passthrough": true
    });
    let body = serde_json::to_vec(&value).expect("serialize body");

    let expected = normalize_official_responses_http_body("/v1/responses", body.clone());
    let (actual, normalized_value) =
        normalize_official_responses_http_body_with_value("/v1/responses", body, value);

    assert_eq!(actual, expected);
    let normalized_value = normalized_value.expect("normalized value");
    assert_eq!(normalized_value["model"], "gpt-5.4");
    assert_eq!(normalized_value["custom_passthrough"], true);
    assert_eq!(normalized_value["stream"], false);
}

#[test]
fn codex_http_rules_promote_and_fill_standard_responses_defaults() {
    let mut obj = serde_json::json!({
        "model": "gpt-5.4",
        "input": [{
            "role": "developer",
            "content": [{"type":"input_text","text":"follow rules"}]
        }],
        "reasoning": {"effort":"high","summary":"auto","context":"current_turn"}
    })
    .as_object()
    .cloned()
    .expect("object");

    let result = apply_codex_http_request_rules(
        "/v1/responses",
        &mut obj,
        true,
        Some("thread-1"),
        false,
        Some("install-1"),
    );

    assert!(result.changed);
    assert_eq!(
        obj.get("instructions").and_then(Value::as_str),
        Some("follow rules")
    );
    assert_eq!(obj.get("stream").and_then(Value::as_bool), Some(true));
    assert_eq!(obj.get("store").and_then(Value::as_bool), Some(false));
    assert_eq!(obj.get("tool_choice").and_then(Value::as_str), Some("auto"));
    assert_eq!(
        obj.get("reasoning")
            .and_then(Value::as_object)
            .and_then(|reasoning| reasoning.get("context"))
            .and_then(Value::as_str),
        Some("current_turn")
    );
    assert_eq!(
        obj.get("reasoning")
            .and_then(Value::as_object)
            .and_then(|reasoning| reasoning.get("summary"))
            .and_then(Value::as_str),
        Some("auto")
    );
    assert_eq!(
        obj.get("prompt_cache_key").and_then(Value::as_str),
        Some("thread-1")
    );
    assert_eq!(
        obj.get("client_metadata")
            .and_then(Value::as_object)
            .and_then(|value| value.get("x-codex-installation-id"))
            .and_then(Value::as_str),
        Some("install-1")
    );
}
