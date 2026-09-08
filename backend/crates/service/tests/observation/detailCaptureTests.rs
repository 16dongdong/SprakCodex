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
