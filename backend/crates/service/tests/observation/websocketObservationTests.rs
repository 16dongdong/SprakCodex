use super::*;

// 构造不带正文的创建消息，验证缺省 generate 表示正常生成而 false 表示预热。
fn create(generate: Option<bool>) -> Message {
    let mut shape = serde_json::json!({"type":"response.create"});
    if let Some(generate) = generate {
        shape["generate"] = generate.into();
    }
    Message::Text(shape.to_string().into())
}

// 构造具有不同响应 ID 的终态，数值均为测试夹具，绝不读取真实会话。
fn complete(identity: &str) -> Message {
    Message::Text(serde_json::json!({"type":"response.completed","response":{"id":identity,"model":"fixture","usage":{"input_tokens":10,"output_tokens":2,"input_tokens_details":{"cached_tokens":0}}}}).to_string().into())
}

// 同连接先预热再生成，两个响应的 request_type 不混淆，也不依赖 output_tokens 是否为零来猜测。
#[test]
fn prewarmAndGenerationRemainDistinct() {
    let mut observer = Observation::default();
    observer
        .request(&create(Some(false)), ("chatgpt.com", "/responses"))
        .unwrap();
    observer
        .request(&create(None), ("chatgpt.com", "/responses"))
        .unwrap();
    let (prewarm, _, _) = observer.response(&complete("first")).unwrap().unwrap();
    let (generation, _, _) = observer.response(&complete("second")).unwrap().unwrap();
    assert_eq!(prewarm.protocol, observationPrewarmRequestType);
    assert_eq!(generation.protocol, generationProtocol);
    assert_eq!(observer.unfinished().count(), 0);
}

// 重放上一终态不应消耗后续创建请求，数据库去重之外也保证连接级元数据对应正确。
#[test]
fn repeatedTerminalDoesNotConsumeNextRequest() {
    let mut observer = Observation::default();
    observer
        .request(&create(Some(false)), ("chatgpt.com", "/responses"))
        .unwrap();
    observer.response(&complete("first")).unwrap().unwrap();
    observer
        .request(&create(Some(true)), ("chatgpt.com", "/responses"))
        .unwrap();
    assert!(observer.response(&complete("first")).unwrap().is_none());
    let (generation, _, _) = observer.response(&complete("second")).unwrap().unwrap();
    assert_eq!(generation.protocol, generationProtocol);
}

// 没有服务端 created 的请求也必须在断开时记录，不能只清理按响应 ID 建立的映射。
#[test]
fn disconnectKeepsQueuedRequests() {
    let mut observer = Observation::default();
    observer
        .request(&create(None), ("chatgpt.com", "/responses"))
        .unwrap();
    assert_eq!(observer.unfinished().count(), 1);
}

// 排队数量有界；超限或没有对应请求的响应返回显式错误，禁止虚构请求属性。
#[test]
fn unmatchedAndOverCapacityMessagesAreErrors() {
    let mut observer = Observation::default();
    assert!(observer.response(&complete("unmatched")).is_err());
    for _ in 0..maxPendingResponses {
        observer
            .request(&create(None), ("chatgpt.com", "/responses"))
            .unwrap();
    }
    assert!(observer
        .request(&create(None), ("chatgpt.com", "/responses"))
        .is_err());
}

// 不携带 response.id 的增量帧仍属于唯一在途响应，必须保留，握手响应头随请求记录交接。
#[test]
fn keepsCompleteFramesAndResponseHeaders() {
    let mut observation = Observation::default();
    observation
        .responseHeaders
        .insert("x-request-id", "fixture-response".parse().unwrap());
    observation
        .request(
            &Message::text(r#"{"type":"response.create","model":"fixture","input":"test"}"#),
            ("api.openai.com", "/v1/responses"),
        )
        .unwrap();
    observation
        .response(&Message::text(
            r#"{"type":"response.created","response":{"id":"response-fixture"}}"#,
        ))
        .unwrap();
    observation
        .response(&Message::text(
            r#"{"type":"response.output_text.delta","delta":"complete-delta"}"#,
        ))
        .unwrap();
    let (exchange, parsed, _) = observation
        .response(&Message::text(
            r#"{"type":"response.completed","response":{"id":"response-fixture"}}"#,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(exchange.method, "GET");
    assert_eq!(exchange.responseHeaders["x-request-id"], "fixture-response");
    let body = parsed.body.snapshot();
    let body = body.as_str().unwrap();
    assert!(body.contains("complete-delta"));
    assert!(body.contains("response.completed"));
}
