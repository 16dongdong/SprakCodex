use super::*;

// 深层凭据与错误中的令牌均需脱敏；普通诊断仍可读。
#[test]
fn masksNestedCredentials() {
    let result = redact(
        json!({"Authorization":"Bearer TOKEN","nested":{"refresh_token":"REFRESH_TOKEN"},"error":"Bearer TOKEN expired","message":"rate limit"}),
    );
    assert_eq!(result["Authorization"], "[已脱敏]");
    assert_eq!(result["nested"]["refresh_token"], "[已脱敏]");
    assert_eq!(result["error"], "[已脱敏] expired");
    assert_eq!(result["message"], "rate limit");
}

// 仓库名、仓库键、会话标识和短前缀文本是普通诊断数据，不得被宽泛凭据规则误伤。
#[test]
fn keepsRepositoryAndOrdinaryMetadataVisible() {
    let result = redact(json!({
        "repo_key":"repository-main",
        "repository":"repo sk-docs rt_notes",
        "session_id":"session-visible",
        "api_key":"sk-1234567890abcdef"
    }));
    assert_eq!(result["repo_key"], "repository-main");
    assert_eq!(result["repository"], "repo sk-docs rt_notes");
    assert_eq!(result["session_id"], "session-visible");
    assert_eq!(result["api_key"], "[已脱敏]");
}

// 大于旧预览上限的完整 JSON 尾部必须可读；临时文件退出后自动清理，不截断正文。
#[test]
fn completeBodyKeepsTailAndDuplicateHeaders() {
    let mut capture = Capture::default();
    let body =
        json!({"text":"x".repeat(512*1024),"tail":"complete","access_token":"TOKEN"}).to_string();
    for chunk in body.as_bytes().chunks(8192) {
        capture.feed(chunk);
    }
    let stored = capture.snapshot();
    assert_eq!(stored["tail"], "complete");
    assert_eq!(stored["access_token"], "[已脱敏]");
    assert_eq!(stored["text"].as_str().unwrap().len(), 512 * 1024);
    let mut headers = hyper::HeaderMap::new();
    headers.append("x-fixture", "one".parse().unwrap());
    headers.append("x-fixture", "two".parse().unwrap());
    assert_eq!(super::headers(&headers)["x-fixture"], json!(["one", "two"]));
}
