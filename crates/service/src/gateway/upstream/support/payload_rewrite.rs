use serde_json::Value;

/// 函数 `body_has_encrypted_content_hint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - in super: 参数 in super
///
/// # 返回
/// 返回函数执行结果
pub(in crate::gateway) fn body_has_encrypted_content_hint(body: &[u8]) -> bool {
    // Fast path: avoid JSON parsing unless we hit a recovery path.
    std::str::from_utf8(body)
        .ok()
        .is_some_and(|text| text.contains("\"encrypted_content\""))
}

/// 函数 `strip_encrypted_content_value`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn item_requires_encrypted_content(value: &Value) -> bool {
    let Value::Object(map) = value else {
        return false;
    };
    // These response-history items are account scoped. Drop the whole item even when a
    // previous recovery pass already removed its encrypted_content field; forwarding the
    // remaining shell makes the Responses API reject input[N].encrypted_content as missing.
    map.get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|item_type| {
            item_type.eq_ignore_ascii_case("reasoning")
                || item_type.eq_ignore_ascii_case("encrypted_content")
                || item_type.eq_ignore_ascii_case("compaction")
                || item_type.eq_ignore_ascii_case("compaction_summary")
                || item_type.eq_ignore_ascii_case("context_compaction")
        })
}

fn strip_encrypted_content_value(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = map.remove("encrypted_content").is_some();
            for child in map.values_mut() {
                if strip_encrypted_content_value(child) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            items.retain_mut(|item| {
                if item_requires_encrypted_content(item) {
                    changed = true;
                    return false;
                }
                if strip_encrypted_content_value(item) {
                    changed = true;
                }
                true
            });
            changed
        }
        _ => false,
    }
}

/// 函数 `strip_encrypted_content_from_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - in super: 参数 in super
///
/// # 返回
/// 返回函数执行结果
pub(in crate::gateway) fn strip_encrypted_content_from_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !strip_encrypted_content_value(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::strip_encrypted_content_from_body;
    use serde_json::{json, Value};

    fn contains_encrypted_content(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key("encrypted_content")
                    || map.values().any(contains_encrypted_content)
            }
            Value::Array(items) => items.iter().any(contains_encrypted_content),
            _ => false,
        }
    }

    #[test]
    fn strip_encrypted_content_removes_items_that_require_the_field() {
        let body = json!({
            "model": "gpt-5.6-sol",
            "encrypted_content": "legacy-root-secret",
            "metadata": {
                "encrypted_content": "metadata-secret",
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"],
                    "encrypted_content": "metadata-reasoning-secret"
                }
            },
            "input": [
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [],
                    "encrypted_content": "reasoning-secret"
                },
                {
                    "type": "compaction",
                    "id": "cmp_1",
                    "encrypted_content": "compaction-secret"
                },
                {
                    "type": "context_compaction",
                    "id": "cmp_2"
                },
                {
                    "type": "agent_message",
                    "content": [
                        { "type": "input_text", "text": "keep me" },
                        {
                            "type": "encrypted_content",
                            "encrypted_content": "nested-secret"
                        }
                    ]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "continue" }]
                }
            ]
        });

        let rewritten = strip_encrypted_content_from_body(
            serde_json::to_vec(&body)
                .expect("serialize body")
                .as_slice(),
        )
        .expect("rewrite body");
        let value: Value = serde_json::from_slice(&rewritten).expect("parse rewritten body");

        assert!(!contains_encrypted_content(&value));
        assert_eq!(
            value["metadata"],
            json!({
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"]
                }
            }),
            "ordinary object properties must remain after their encrypted field is removed"
        );

        let input = value["input"].as_array().expect("input array");
        assert_eq!(input.len(), 2, "reasoning item must be removed");
        assert_eq!(input[0]["type"], "agent_message");
        assert_eq!(
            input[0]["content"],
            json!([{ "type": "input_text", "text": "keep me" }]),
            "encrypted content part must be removed without dropping normal text"
        );
        assert_eq!(input[1]["type"], "message");
    }

    #[test]
    fn strip_encrypted_content_removes_compaction_at_reported_input_index() {
        let mut input = (0..24)
            .map(|index| {
                json!({
                    "type": "message",
                    "role": "user",
                    "content": [{
                        "type": "input_text",
                        "text": format!("history-{index}")
                    }]
                })
            })
            .collect::<Vec<_>>();
        input.push(json!({
            "type": "compaction",
            "id": "cmp_reported_24",
            "encrypted_content": "account-scoped-compaction"
        }));
        input.push(json!({
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": "continue" }]
        }));
        let body = json!({
            "model": "gpt-5.6-sol",
            "input": input
        });

        let rewritten = strip_encrypted_content_from_body(
            serde_json::to_vec(&body)
                .expect("serialize body")
                .as_slice(),
        )
        .expect("rewrite body");
        let value: Value = serde_json::from_slice(&rewritten).expect("parse rewritten body");
        let input = value["input"].as_array().expect("input array");

        assert_eq!(input.len(), 25);
        assert_eq!(input[24]["type"], "message");
        assert_eq!(input[24]["content"][0]["text"], "continue");
    }
}
