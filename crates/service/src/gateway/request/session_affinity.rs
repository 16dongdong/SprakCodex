#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutgoingSessionAffinity<'a> {
    pub(crate) incoming_session_id: Option<&'a str>,
    pub(crate) incoming_client_request_id: Option<&'a str>,
    pub(crate) incoming_turn_state: Option<&'a str>,
    pub(crate) fallback_session_id: Option<&'a str>,
}

/// 函数 `normalize_anchor`
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
fn normalize_anchor(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn anchor_fingerprint_or_dash(value: Option<&str>) -> String {
    normalize_anchor(value)
        .map(super::anchor_fingerprint::fingerprint_anchor)
        .unwrap_or_else(|| "-".to_string())
}

pub(crate) fn log_outgoing_session_affinity(
    context: &str,
    account_id: Option<&str>,
    incoming_session_id: Option<&str>,
    incoming_client_request_id: Option<&str>,
    incoming_turn_state: Option<&str>,
    conversation_id: Option<&str>,
    prompt_cache_key: Option<&str>,
    outgoing: OutgoingSessionAffinity<'_>,
    strip_session_affinity: bool,
) {
    let incoming_session = normalize_anchor(incoming_session_id);
    let incoming_client_request = normalize_anchor(incoming_client_request_id);
    let incoming_turn_state = normalize_anchor(incoming_turn_state);
    let resolved_session = normalize_anchor(outgoing.incoming_session_id);
    let resolved_client_request = normalize_anchor(outgoing.incoming_client_request_id);
    let resolved_turn_state = normalize_anchor(outgoing.incoming_turn_state);
    let fallback_session = normalize_anchor(outgoing.fallback_session_id);

    let session_rewritten = incoming_session != resolved_session;
    let client_request_rewritten = incoming_client_request != resolved_client_request;
    let turn_state_rewritten = incoming_turn_state != resolved_turn_state;
    let should_log = strip_session_affinity
        || session_rewritten
        || client_request_rewritten
        || turn_state_rewritten
        || fallback_session.is_some();
    if !should_log {
        return;
    }

    log::info!(
        "event=gateway_session_affinity_resolved context={} account_id={} strip_session_affinity={} session_rewritten={} client_request_rewritten={} turn_state_rewritten={} incoming_session_fp={} resolved_session_fp={} incoming_client_request_fp={} resolved_client_request_fp={} incoming_turn_state_present={} resolved_turn_state_present={} conversation_fp={} prompt_cache_key_fp={} fallback_session_fp={}",
        context,
        account_id.unwrap_or("-"),
        if strip_session_affinity { "true" } else { "false" },
        if session_rewritten { "true" } else { "false" },
        if client_request_rewritten { "true" } else { "false" },
        if turn_state_rewritten { "true" } else { "false" },
        anchor_fingerprint_or_dash(incoming_session),
        anchor_fingerprint_or_dash(resolved_session),
        anchor_fingerprint_or_dash(incoming_client_request),
        anchor_fingerprint_or_dash(resolved_client_request),
        if incoming_turn_state.is_some() { "true" } else { "false" },
        if resolved_turn_state.is_some() { "true" } else { "false" },
        anchor_fingerprint_or_dash(conversation_id),
        anchor_fingerprint_or_dash(prompt_cache_key),
        anchor_fingerprint_or_dash(fallback_session),
    );
}

/// 函数 `derive_outgoing_session_affinity`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn derive_outgoing_session_affinity<'a>(
    incoming_session_id: Option<&'a str>,
    incoming_client_request_id: Option<&'a str>,
    incoming_turn_state: Option<&'a str>,
    conversation_id: Option<&'a str>,
) -> OutgoingSessionAffinity<'a> {
    let mut resolved_turn_state = incoming_turn_state;
    let conversation_anchor = normalize_anchor(conversation_id);
    let resolved_client_request_id = conversation_anchor.or(incoming_client_request_id);

    if resolved_turn_state.is_some()
        && normalize_anchor(incoming_session_id).is_none()
        && conversation_anchor.is_none()
    {
        // 中文注释：没有任何稳定线程锚点时，孤儿 turn-state 不应继续透传。
        resolved_turn_state = None;
    }

    OutgoingSessionAffinity {
        // 中文注释：session-id 表示根会话，不能被父/子线程 ID 或缓存键覆盖。
        incoming_session_id: normalize_anchor(incoming_session_id),
        incoming_client_request_id: resolved_client_request_id,
        incoming_turn_state: resolved_turn_state,
        // 中文注释：conversation/thread 只在缺少显式 session-id 时提供兼容回退。
        fallback_session_id: conversation_anchor,
    }
}

#[cfg(test)]
#[path = "session_affinity_tests.rs"]
mod tests;
