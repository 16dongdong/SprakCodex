use super::resolve_direct_upstream_model_for_log;

#[test]
fn direct_upstream_model_is_logged_for_override() {
    assert_eq!(
        resolve_direct_upstream_model_for_log(Some("gpt-5"), Some("gpt-5.4-openai-compact"),),
        Some("gpt-5.4-openai-compact")
    );
}

#[test]
fn direct_upstream_model_is_ignored_when_same_as_platform_model() {
    assert_eq!(
        resolve_direct_upstream_model_for_log(Some("gpt-5"), Some("gpt-5")),
        None
    );
}
