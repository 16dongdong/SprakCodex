use codexmanager_core::storage::ManagedModelV2;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstructionProtocolV2 {
    OpenAi,
    Anthropic,
    Gemini,
}

fn non_empty_text(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn content_has_client_instruction(content: Option<&Value>) -> bool {
    match content {
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(parts)) => parts.iter().any(|part| match part {
            Value::String(text) => !text.trim().is_empty(),
            Value::Object(object) => {
                object
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty())
                    || !object.is_empty()
            }
            _ => true,
        }),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn leading_instruction_count(items: Option<&Value>) -> usize {
    let Some(items) = items.and_then(Value::as_array) else {
        return 0;
    };
    items
        .iter()
        .take_while(|item| {
            item.as_object()
                .and_then(|object| object.get("role"))
                .and_then(Value::as_str)
                .is_some_and(|role| matches!(role, "system" | "developer"))
        })
        .count()
}

fn leading_has_content(items: Option<&Value>) -> bool {
    let Some(items) = items.and_then(Value::as_array) else {
        return false;
    };
    items
        .iter()
        .take(leading_instruction_count(Some(&Value::Array(
            items.clone(),
        ))))
        .any(|item| content_has_client_instruction(item.get("content")))
}

fn remove_leading_instructions(object: &mut Map<String, Value>, key: &str) {
    let count = leading_instruction_count(object.get(key));
    if count == 0 {
        return;
    }
    if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
        items.drain(0..count);
    }
}

fn anthropic_system_has_content(value: Option<&Value>) -> bool {
    content_has_client_instruction(value)
}

fn gemini_system_has_content(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .and_then(|object| object.get("parts"))
        .and_then(Value::as_array)
        .is_some_and(|parts| {
            parts.iter().any(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty())
            })
        })
}

pub(crate) fn apply_model_instructions_v2(
    body: &mut Value,
    model: &ManagedModelV2,
    protocol: InstructionProtocolV2,
) -> Result<(), String> {
    let object = body
        .as_object_mut()
        .ok_or_else(|| "request body must be an object".to_string())?;
    let has_client_content = match protocol {
        InstructionProtocolV2::OpenAi => {
            non_empty_text(object.get("instructions"))
                || leading_has_content(object.get("input"))
                || leading_has_content(object.get("messages"))
        }
        InstructionProtocolV2::Anthropic => anthropic_system_has_content(object.get("system")),
        InstructionProtocolV2::Gemini => gemini_system_has_content(object.get("systemInstruction")),
    };
    match model.instructions_mode.as_str() {
        "passthrough" => Ok(()),
        "fallback" if has_client_content => Ok(()),
        "fallback" => {
            let Some(text) = model
                .instructions_text
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            else {
                return Ok(());
            };
            set_instruction_text(object, protocol, text, false)
        }
        "override" => {
            let text = model
                .instructions_text
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| "override instructions require non-empty text".to_string())?;
            set_instruction_text(object, protocol, text, true)
        }
        _ => Err("invalid model instructions mode".to_string()),
    }
}

fn set_instruction_text(
    object: &mut Map<String, Value>,
    protocol: InstructionProtocolV2,
    text: &str,
    remove_leading: bool,
) -> Result<(), String> {
    match protocol {
        InstructionProtocolV2::OpenAi => {
            object.insert("instructions".to_string(), Value::String(text.to_string()));
            if remove_leading {
                remove_leading_instructions(object, "input");
                remove_leading_instructions(object, "messages");
            }
        }
        InstructionProtocolV2::Anthropic => {
            object.insert("system".to_string(), Value::String(text.to_string()));
        }
        InstructionProtocolV2::Gemini => {
            object.insert(
                "systemInstruction".to_string(),
                serde_json::json!({"parts":[{"text":text}]}),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(mode: &str, text: Option<&str>) -> ManagedModelV2 {
        ManagedModelV2 {
            instructions_mode: mode.to_string(),
            instructions_text: text.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn fallback_preserves_non_empty_client_text_byte_for_byte() {
        let original = "  client text\n";
        let mut body = serde_json::json!({"instructions":original});
        apply_model_instructions_v2(
            &mut body,
            &model("fallback", Some("model text")),
            InstructionProtocolV2::OpenAi,
        )
        .unwrap();
        assert_eq!(body["instructions"], original);
    }

    #[test]
    fn fallback_uses_model_text_only_when_all_client_channels_are_empty() {
        let mut body =
            serde_json::json!({"instructions":"  ","input":[{"role":"user","content":"hi"}]});
        apply_model_instructions_v2(
            &mut body,
            &model("fallback", Some("model text")),
            InstructionProtocolV2::OpenAi,
        )
        .unwrap();
        assert_eq!(body["instructions"], "model text");
    }

    #[test]
    fn override_replaces_top_level_and_removes_only_leading_instruction_messages() {
        let mut body = serde_json::json!({"instructions":"client","input":[
            {"role":"system","content":"leading"},{"role":"developer","content":"leading 2"},
            {"role":"user","content":"hi"},{"role":"system","content":"history"}]});
        apply_model_instructions_v2(
            &mut body,
            &model("override", Some("model text")),
            InstructionProtocolV2::OpenAi,
        )
        .unwrap();
        assert_eq!(body["instructions"], "model text");
        assert_eq!(body["input"].as_array().unwrap().len(), 2);
        assert_eq!(body["input"][1]["content"], "history");
    }

    #[test]
    fn anthropic_and_gemini_use_native_system_channels() {
        let mut anthropic = serde_json::json!({});
        let mut gemini = serde_json::json!({});
        let policy = model("fallback", Some("native text"));
        apply_model_instructions_v2(&mut anthropic, &policy, InstructionProtocolV2::Anthropic)
            .unwrap();
        apply_model_instructions_v2(&mut gemini, &policy, InstructionProtocolV2::Gemini).unwrap();
        assert_eq!(anthropic["system"], "native text");
        assert_eq!(
            gemini["systemInstruction"]["parts"][0]["text"],
            "native text"
        );
    }
}
