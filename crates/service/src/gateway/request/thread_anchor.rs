use codexmanager_core::storage::ConversationBinding;

use super::incoming_headers::IncomingHeaderSnapshot;

fn normalize_anchor(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn has_native_thread_anchor(headers: &IncomingHeaderSnapshot) -> bool {
    normalize_anchor(headers.conversation_id()).is_some()
        || normalize_anchor(headers.turn_state()).is_some()
}

pub(crate) fn resolve_local_conversation_id_with_sticky_fallback(
    headers: &IncomingHeaderSnapshot,
    allow_sticky_fallback: bool,
) -> Option<String> {
    normalize_anchor(headers.conversation_id()).or_else(|| {
        if !allow_sticky_fallback || normalize_anchor(headers.turn_state()).is_some() {
            return None;
        }
        super::upstream::header_profile::derive_sticky_conversation_id_from_headers(headers)
    })
}

pub(crate) fn resolve_fallback_thread_anchor(
    headers: &IncomingHeaderSnapshot,
    local_conversation_id: Option<&str>,
    binding: Option<&ConversationBinding>,
) -> Option<String> {
    if has_native_thread_anchor(headers) {
        return None;
    }
    super::conversation_binding::effective_thread_anchor(local_conversation_id, binding)
}

#[cfg(test)]
#[path = "thread_anchor_tests.rs"]
mod tests;
