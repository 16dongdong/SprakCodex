use super::*;
use std::thread;
use std::time::Duration;

struct PausingReader {
    payload: Cursor<Vec<u8>>,
    paused: bool,
}

impl PausingReader {
    fn new(payload: &str) -> Self {
        Self {
            payload: Cursor::new(payload.as_bytes().to_vec()),
            paused: false,
        }
    }
}

impl Read for PausingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.payload.read(buf)?;
        if read > 0 {
            return Ok(read);
        }
        if !self.paused {
            self.paused = true;
            thread::sleep(Duration::from_millis(200));
        }
        Ok(0)
    }
}

#[test]
fn metadata_only_upstream_frame_records_first_response_before_keepalive() {
    let _guard = crate::test_env_guard();
    let _runtime_guard = super::super::SseKeepaliveRuntimeGuard::enabled_with_interval(1);
    let upstream = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_first\",\"model\":\"gpt-5.4\"}}\n\n",
    );
    let usage_collector = Arc::new(Mutex::new(UpstreamResponseUsage::default()));
    let mut reader = AnthropicSseReader::from_reader(
        PausingReader::new(upstream),
        Arc::clone(&usage_collector),
        Some("fallback-model"),
        None,
        Instant::now(),
    );
    let mut buf = [0_u8; 4096];

    let read = reader.read(&mut buf).expect("read keepalive");

    assert!(read > 0);
    assert_eq!(
        std::str::from_utf8(&buf[..read]).expect("utf8"),
        std::str::from_utf8(SseKeepAliveFrame::Anthropic.bytes()).expect("utf8")
    );
    let usage = usage_collector.lock().expect("usage lock").clone();
    assert!(usage.first_response_ms.is_some());
}
