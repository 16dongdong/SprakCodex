use super::derive_outgoing_session_affinity;

/// 函数 `uses_conversation_anchor_when_prompt_cache_missing`
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
fn uses_conversation_anchor_when_prompt_cache_missing() {
    let actual = derive_outgoing_session_affinity(
        Some("legacy_session_should_not_win"),
        Some("legacy_request_id_should_not_win"),
        Some("legacy_turn_state_should_not_win"),
        Some("conv_anchor_only"),
    );

    assert_eq!(
        actual.incoming_session_id,
        Some("legacy_session_should_not_win")
    );
    assert_eq!(actual.incoming_client_request_id, Some("conv_anchor_only"));
    assert_eq!(
        actual.incoming_turn_state,
        Some("legacy_turn_state_should_not_win")
    );
    assert_eq!(actual.fallback_session_id, Some("conv_anchor_only"));
}

/// 函数 `uses_thread_anchor_for_fallback_headers`
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
fn uses_thread_anchor_for_fallback_headers() {
    let actual = derive_outgoing_session_affinity(
        Some("legacy_session_should_not_win"),
        Some("legacy_request_id_should_not_win"),
        Some("legacy_turn_state_should_not_win"),
        Some("conv_anchor_fallback"),
    );

    assert_eq!(
        actual.incoming_session_id,
        Some("legacy_session_should_not_win")
    );
    assert_eq!(
        actual.incoming_client_request_id,
        Some("conv_anchor_fallback")
    );
    assert_eq!(
        actual.incoming_turn_state,
        Some("legacy_turn_state_should_not_win")
    );
    assert_eq!(actual.fallback_session_id, Some("conv_anchor_fallback"));
}

/// 函数 `clears_turn_state_when_thread_anchor_diverges`
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
fn preserves_root_session_and_turn_state_for_child_thread() {
    let actual = derive_outgoing_session_affinity(
        Some("root_session"),
        Some("legacy_request_id_should_not_win"),
        Some("parent_turn_state"),
        Some("child_thread"),
    );

    assert_eq!(actual.incoming_session_id, Some("root_session"));
    assert_eq!(actual.incoming_client_request_id, Some("child_thread"));
    assert_eq!(actual.incoming_turn_state, Some("parent_turn_state"));
    assert_eq!(actual.fallback_session_id, Some("child_thread"));
}

/// 函数 `drops_orphan_turn_state_without_conversation_anchor`
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
fn drops_orphan_turn_state_without_conversation_anchor() {
    let actual = derive_outgoing_session_affinity(
        None,
        Some("explicit_client_request_id"),
        Some("turn_state_ok"),
        None,
    );

    assert_eq!(actual.incoming_session_id, None);
    assert_eq!(
        actual.incoming_client_request_id,
        Some("explicit_client_request_id")
    );
    assert_eq!(actual.incoming_turn_state, None);
    assert_eq!(actual.fallback_session_id, None);
}

/// 函数 `uses_conversation_as_session_fallback_without_rewriting_explicit_fields`
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
fn uses_conversation_as_session_fallback_without_rewriting_explicit_fields() {
    let actual = derive_outgoing_session_affinity(
        None,
        Some("client_request"),
        Some("turn_state"),
        Some("thread_anchor"),
    );

    assert_eq!(actual.incoming_session_id, None);
    assert_eq!(actual.incoming_client_request_id, Some("thread_anchor"));
    assert_eq!(actual.incoming_turn_state, Some("turn_state"));
    assert_eq!(actual.fallback_session_id, Some("thread_anchor"));
}
