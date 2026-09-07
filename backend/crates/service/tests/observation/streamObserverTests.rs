use super::*;
use tokio::io::AsyncWriteExt;

const event: &[u8] = b"event: response.completed\r\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"fixture\",\"model\":\"fixture\",\"usage\":{\"input_tokens\":20,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":5}}}}\r\n\r\n";

// 未声明 SSE 的真实 framing 仍应正确解析，而且头字段跨字节分包不能丢失。
#[tokio::test]
async fn missingSseContentTypeStillParsesFragmentedFrames() {
    let (mut writer, reader) = tokio::io::duplex(1);
    let sending = tokio::spawn(async move {
        writer.write_all(event).await.unwrap();
    });
    let parsed = consumeDecoded(Box::pin(reader), false).await;
    sending.await.unwrap();
    assert_eq!(parsed.usage.input_tokens, Some(20));
    assert_eq!(parsed.usage.cached_input_tokens, Some(5));
    assert_eq!(parsed.usage.output_tokens, Some(3));
    assert!(parsed.terminal);
}

// 结构探测不应破坏普通 JSON，也不能把 JSON 中的 data: 文本当 SSE framing。
#[tokio::test]
async fn jsonUsesItsActualStructure() {
    let body =
        br#"{"id":"fixture","usage":{"input_tokens":7,"output_tokens":2},"ignored":"data:"}"#;
    let parsed = consumeDecoded(Box::pin(std::io::Cursor::new(body.to_vec())), false).await;
    assert_eq!(parsed.usage.total_tokens, Some(9));
}

// SSE 允许 UTF-8 BOM；只从解码副本剥离标记，客户端收到的原字节保持不变。
#[tokio::test]
async fn sseBomIsAccepted() {
    let mut payload = b"\xef\xbb\xbf".to_vec();
    payload.extend_from_slice(event);
    let parsed = consumeDecoded(Box::pin(std::io::Cursor::new(payload)), false).await;
    assert_eq!(parsed.usage.total_tokens, Some(23));
}
