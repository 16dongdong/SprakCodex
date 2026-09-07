use super::is_removed_openai_compat_request_path;

#[test]
fn identifies_removed_openai_compat_paths() {
    assert!(!is_removed_openai_compat_request_path("/v1/responses"));
    assert!(!is_removed_openai_compat_request_path(
        "/v1/responses/compact"
    ));
    assert!(!is_removed_openai_compat_request_path(
        "/v1/chat/completions"
    ));
    assert!(is_removed_openai_compat_request_path("/v1/completions"));
    assert!(!is_removed_openai_compat_request_path("/v1/messages"));
    assert!(!is_removed_openai_compat_request_path(
        "/v1beta/models/gemini-2.5-pro:generateContent"
    ));
    assert!(!is_removed_openai_compat_request_path(
        "/v1/images/generations"
    ));
    assert!(!is_removed_openai_compat_request_path("/v1/images/edits"));
}

#[test]
fn removed_openai_compat_paths_are_still_limited_to_legacy_completions() {
    assert!(is_removed_openai_compat_request_path("/v1/completions"));
    assert!(!is_removed_openai_compat_request_path("/v1/messages"));
    assert!(!is_removed_openai_compat_request_path(
        "/v1beta/models/gemini-2.5-pro:generateContent"
    ));
}
