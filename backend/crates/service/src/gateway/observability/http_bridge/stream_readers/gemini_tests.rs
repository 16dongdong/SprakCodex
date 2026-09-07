use super::*;
use std::io::Read;
use std::time::Instant;

#[test]
fn responses_reasoning_summary_streams_as_gemini_thought() {
    let upstream = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-5.4\",\"created_at\":1775900000}}\n\n",
        "event: response.reasoning_summary_text.delta\n",
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"plan details\"}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-5.4\",\"created_at\":1775900000,\"usage\":{\"input_tokens\":3,\"output_tokens\":4,\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
    );
    let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
    let mut reader = GeminiSseReader::from_reader(
        Cursor::new(upstream.as_bytes().to_vec()),
        usage_collector,
        None,
        GeminiStreamOutputMode::Sse,
        true,
        Instant::now(),
    );

    let mut output = String::new();
    reader
        .read_to_string(&mut output)
        .expect("read adapted gemini stream");

    assert!(output.contains("\"thought\":true"));
    assert!(output.contains("\"text\":\"plan details\""));
    assert!(output.contains("\"thoughtsTokenCount\":2"));
    assert!(output.contains("\"candidates\":"));
    assert!(output.contains("\"response\":{"));
}

#[test]
fn gemini_cli_stream_wraps_payload_in_response_envelope() {
    let upstream = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-5.4\",\"created_at\":1775900000}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-5.4\",\"created_at\":1775900000,\"usage\":{\"input_tokens\":3,\"output_tokens\":4,\"total_tokens\":7}}}\n\n",
    );
    let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
    let mut reader = GeminiSseReader::from_reader(
        Cursor::new(upstream.as_bytes().to_vec()),
        Arc::clone(&usage_collector),
        None,
        GeminiStreamOutputMode::Sse,
        true,
        Instant::now(),
    );

    let mut output = String::new();
    reader
        .read_to_string(&mut output)
        .expect("read wrapped gemini stream");

    assert!(output.starts_with("data: {\"response\":{"));
    assert!(output.contains("\"candidates\":"));
    assert!(output.contains("\"responseId\":\"resp_test\""));
}

#[test]
fn completed_reasoning_fallback_preserves_thought_signature() {
    let response = json!({
        "output": [{
            "type": "reasoning",
            "encrypted_content": "sig",
            "summary": [{ "type": "summary_text", "text": "hidden plan" }]
        }]
    });

    let reasoning = extract_completed_response_reasoning(&response).expect("reasoning");
    assert_eq!(reasoning.text, "hidden plan");
    assert_eq!(reasoning.encrypted_content.as_deref(), Some("sig"));
}
