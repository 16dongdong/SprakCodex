use super::*;
use base64::Engine;

// 展示身份来自同一请求的令牌声明，但原始令牌不进入记录；名称缺失时不伪装成密钥名称。
#[test]
fn extractsEmailAndNameWithoutKeepingCredential() {
    let claims = serde_json::json!({"sub":"fixture-user","https://api.openai.com/profile":{"name":"Fixture Name","email":"fixture@example.com"}});
    let token = format!(
        "e30.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let mut headers = hyper::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    headers.insert("chatgpt-account-id", "fixture-account".parse().unwrap());
    let mut exchange = Exchange::new("chatgpt.com", "/backend-api/codex/responses", "websocket");
    exchange.captureHeaders(&headers);
    assert_eq!(
        exchange.accountLabel.as_deref(),
        Some("Fixture Name <fixture@example.com>")
    );
    assert_eq!(exchange.account.as_deref(), Some("fixture-account"));
    assert_eq!(exchange.requestHeaders["authorization"], "[已脱敏]");
    assert!(!exchange.requestHeaders.to_string().contains(&token));
}
