use std::sync::mpsc;
use std::time::Duration;

use bytes::Bytes;

use super::*;
use crate::gateway::http_bridge::{
    OpenAIResponsesPassthroughSseReader, PassthroughSseCollector, SseKeepAliveFrame,
};

#[test]
fn prefetch_caps_the_classification_copy_and_replays_the_full_chunk() {
    let body = Bytes::from(vec![b'x'; 128 * 1024]);
    let stream = GatewayByteStream::from_bytes(body.clone());

    let (prefix, replayed, terminal) =
        stream.prefetch_until(64 * 1024, Some(Duration::from_secs(1)), None, |_| false);

    assert_eq!(prefix.len(), 64 * 1024);
    assert_eq!(terminal, GatewayStreamPrefetchTerminal::PrefixLimit);
    assert_eq!(replayed.read_all_bytes().expect("replayed body"), body);
}

#[test]
fn prefetch_reports_eof_without_losing_the_buffered_body() {
    let body = Bytes::from_static(b"metadata only");
    let stream = GatewayByteStream::from_bytes(body.clone());

    let (prefix, replayed, terminal) =
        stream.prefetch_until(1024, Some(Duration::from_secs(1)), None, |_| false);

    assert_eq!(prefix, body);
    assert_eq!(terminal, GatewayStreamPrefetchTerminal::Eof);
    assert_eq!(replayed.read_all_bytes().expect("replayed body"), body);
}

#[test]
fn prefetch_reports_and_replays_stream_errors() {
    let (tx, rx) = mpsc::sync_channel(2);
    tx.send(GatewayByteStreamItem::Chunk(Bytes::from_static(
        b"metadata",
    )))
    .expect("send metadata");
    tx.send(GatewayByteStreamItem::Error("upstream reset".to_string()))
        .expect("send stream error");
    let stream = GatewayByteStream::from_receiver(rx);

    let (prefix, replayed, terminal) =
        stream.prefetch_until(1024, Some(Duration::from_secs(1)), None, |_| false);

    assert_eq!(prefix.as_ref(), b"metadata");
    assert_eq!(
        terminal,
        GatewayStreamPrefetchTerminal::Error("upstream reset".to_string())
    );
    assert_eq!(replayed.read_all_bytes(), Err("upstream reset".to_string()));
}

#[test]
fn prefetch_distinguishes_a_disconnected_producer_from_clean_eof() {
    let (tx, rx) = mpsc::sync_channel(1);
    drop(tx);
    let stream = GatewayByteStream::from_receiver(rx);

    let (prefix, _replayed, terminal) =
        stream.prefetch_until(1024, Some(Duration::from_secs(1)), None, |_| false);

    assert!(prefix.is_empty());
    assert_eq!(terminal, GatewayStreamPrefetchTerminal::Disconnected);
}

#[test]
fn prefetch_wall_clock_timeout_is_not_reset_by_activity_and_replays_all_bytes() {
    let (tx, rx) = mpsc::sync_channel(32);
    let producer = std::thread::spawn(move || {
        let mut expected = Vec::new();
        for index in 0..20 {
            let chunk = format!("chunk-{index};").into_bytes();
            expected.extend_from_slice(chunk.as_slice());
            tx.send(GatewayByteStreamItem::Chunk(Bytes::from(chunk)))
                .expect("send active stream chunk");
            std::thread::sleep(Duration::from_millis(5));
        }
        tx.send(GatewayByteStreamItem::Eof)
            .expect("send active stream EOF");
        expected
    });
    let stream = GatewayByteStream::from_receiver(rx);

    let started_at = std::time::Instant::now();
    let (_prefix, replayed, terminal) = stream.prefetch_until(
        1024,
        Some(Duration::from_secs(1)),
        Some(Duration::from_millis(30)),
        |_| false,
    );

    assert_eq!(terminal, GatewayStreamPrefetchTerminal::WallClockTimeout);
    assert!(started_at.elapsed() < Duration::from_millis(750));
    let expected = producer.join().expect("join active stream producer");
    assert_eq!(replayed.read_all_bytes().expect("replayed body"), expected);
}

#[test]
fn prefetch_idle_timeout_wins_before_later_wall_clock_timeout() {
    let (_tx, rx) = mpsc::sync_channel(1);
    let stream = GatewayByteStream::from_receiver(rx);

    let (_prefix, _replayed, terminal) = stream.prefetch_until(
        1024,
        Some(Duration::from_millis(20)),
        Some(Duration::from_millis(200)),
        |_| false,
    );

    assert_eq!(terminal, GatewayStreamPrefetchTerminal::IdleTimeout);
}

#[test]
fn dropping_a_stream_signals_its_upstream_producer_to_cancel() {
    let (_tx, rx) = mpsc::sync_channel(1);
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
    let stream = GatewayByteStream::from_receiver_with_cancel(rx, Some(cancel_tx));

    drop(stream);

    assert_eq!(cancel_rx.try_recv(), Ok(()));
}

#[test]
fn dropping_both_tee_consumers_cancels_a_silent_upstream() {
    let (_source_tx, source_rx) = mpsc::sync_channel(1);
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
    let source = GatewayByteStream::from_receiver_with_cancel(source_rx, Some(cancel_tx));
    let (left, right) = source.tee();

    drop(left);
    drop(right);

    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match cancel_rx.try_recv() {
            Ok(()) => break,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                if std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => panic!("tee relay did not cancel silent upstream: {other:?}"),
        }
    }
}

#[test]
fn dropping_openai_responses_reader_cancels_sidecar_and_silent_upstream() {
    let (_source_tx, source_rx) = mpsc::sync_channel(1);
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
    let source = GatewayByteStream::from_receiver_with_cancel(source_rx, Some(cancel_tx));
    let response = GatewayStreamResponse::new(
        reqwest::StatusCode::OK,
        reqwest::header::HeaderMap::new(),
        source,
    );
    let reader = OpenAIResponsesPassthroughSseReader::from_stream_response(
        response,
        std::sync::Arc::new(std::sync::Mutex::new(PassthroughSseCollector::default())),
        SseKeepAliveFrame::Comment,
        std::time::Instant::now(),
    );

    drop(reader);

    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match cancel_rx.try_recv() {
            Ok(()) => break,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                if std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => panic!("responses reader did not cancel silent upstream: {other:?}"),
        }
    }
}
