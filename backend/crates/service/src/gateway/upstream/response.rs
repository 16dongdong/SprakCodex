use bytes::Bytes;
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const GATEWAY_STREAM_READ_CHUNK_BYTES: usize = 8 * 1024;
const GATEWAY_STREAM_CHANNEL_CAPACITY: usize = 128;
const GATEWAY_STREAM_TEE_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub(crate) enum GatewayByteStreamItem {
    Chunk(Bytes),
    Eof,
    Error(String),
}

#[derive(Debug)]
pub(crate) struct GatewayByteStream {
    rx: Receiver<GatewayByteStreamItem>,
    replay: VecDeque<GatewayByteStreamItem>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    tee_consumers: Option<Arc<AtomicUsize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GatewayStreamPrefetchTerminal {
    Open,
    PrefixLimit,
    IdleTimeout,
    WallClockTimeout,
    Eof,
    Error(String),
    Disconnected,
}

impl GatewayByteStream {
    pub(crate) fn from_bytes(body: Bytes) -> Self {
        let (tx, rx) = mpsc::sync_channel::<GatewayByteStreamItem>(2);
        if !body.is_empty() {
            let _ = tx.send(GatewayByteStreamItem::Chunk(body));
        }
        let _ = tx.send(GatewayByteStreamItem::Eof);
        Self::from_receiver(rx)
    }

    pub(crate) fn from_blocking_response(mut response: reqwest::blocking::Response) -> Self {
        let (tx, rx) = mpsc::sync_channel::<GatewayByteStreamItem>(GATEWAY_STREAM_CHANNEL_CAPACITY);
        thread::spawn(move || loop {
            let mut buffer = vec![0_u8; GATEWAY_STREAM_READ_CHUNK_BYTES];
            match response.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(GatewayByteStreamItem::Eof);
                    return;
                }
                Ok(read) => {
                    buffer.truncate(read);
                    if tx
                        .send(GatewayByteStreamItem::Chunk(Bytes::from(buffer)))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(err) => {
                    let _ = tx.send(GatewayByteStreamItem::Error(err.to_string()));
                    return;
                }
            }
        });
        Self::from_receiver(rx)
    }

    pub(crate) fn from_receiver(rx: Receiver<GatewayByteStreamItem>) -> Self {
        Self::from_receiver_with_cancel(rx, None)
    }

    pub(crate) fn from_receiver_with_cancel(
        rx: Receiver<GatewayByteStreamItem>,
        cancel: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> Self {
        Self::from_receiver_with_parts(rx, cancel, None)
    }

    fn from_receiver_with_parts(
        rx: Receiver<GatewayByteStreamItem>,
        cancel: Option<tokio::sync::oneshot::Sender<()>>,
        tee_consumers: Option<Arc<AtomicUsize>>,
    ) -> Self {
        Self {
            rx,
            replay: VecDeque::new(),
            cancel,
            tee_consumers,
        }
    }

    pub(crate) fn recv(&mut self) -> Result<GatewayByteStreamItem, mpsc::RecvError> {
        if let Some(item) = self.replay.pop_front() {
            return Ok(item);
        }
        self.rx.recv()
    }

    pub(crate) fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<GatewayByteStreamItem, RecvTimeoutError> {
        if let Some(item) = self.replay.pop_front() {
            return Ok(item);
        }
        self.rx.recv_timeout(timeout)
    }

    fn prefetch_until<F>(
        mut self,
        max_bytes: usize,
        idle_timeout: Option<Duration>,
        wall_clock_timeout: Option<Duration>,
        should_stop: F,
    ) -> (Bytes, Self, GatewayStreamPrefetchTerminal)
    where
        F: Fn(&[u8]) -> bool,
    {
        let mut prefix = Vec::new();
        let mut replay = VecDeque::new();
        let mut terminal = GatewayStreamPrefetchTerminal::Open;
        let started_at = Instant::now();

        loop {
            if prefix.len() >= max_bytes {
                terminal = GatewayStreamPrefetchTerminal::PrefixLimit;
                break;
            }
            if should_stop(prefix.as_slice()) {
                break;
            }
            let wall_clock_remaining =
                wall_clock_timeout.map(|timeout| timeout.saturating_sub(started_at.elapsed()));
            if wall_clock_remaining.is_some_and(|remaining| remaining.is_zero()) {
                terminal = GatewayStreamPrefetchTerminal::WallClockTimeout;
                break;
            }
            let recv_timeout = match (idle_timeout, wall_clock_remaining) {
                (Some(idle), Some(wall_clock)) => Some(idle.min(wall_clock)),
                (Some(idle), None) => Some(idle),
                (None, Some(wall_clock)) => Some(wall_clock),
                (None, None) => None,
            };
            let next_item = match recv_timeout {
                Some(timeout) => self.recv_timeout(timeout),
                None => self.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match next_item {
                Ok(item @ GatewayByteStreamItem::Chunk(_)) => {
                    if let GatewayByteStreamItem::Chunk(bytes) = &item {
                        let remaining_bytes = max_bytes.saturating_sub(prefix.len());
                        let copy_len = remaining_bytes.min(bytes.len());
                        prefix.extend_from_slice(&bytes[..copy_len]);
                    }
                    replay.push_back(item);
                }
                Ok(item @ GatewayByteStreamItem::Eof) => {
                    terminal = GatewayStreamPrefetchTerminal::Eof;
                    replay.push_back(item);
                    break;
                }
                Ok(item @ GatewayByteStreamItem::Error(_)) => {
                    if let GatewayByteStreamItem::Error(err) = &item {
                        terminal = GatewayStreamPrefetchTerminal::Error(err.clone());
                    }
                    replay.push_back(item);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    terminal = if wall_clock_timeout
                        .is_some_and(|timeout| started_at.elapsed() >= timeout)
                    {
                        GatewayStreamPrefetchTerminal::WallClockTimeout
                    } else {
                        GatewayStreamPrefetchTerminal::IdleTimeout
                    };
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    terminal = GatewayStreamPrefetchTerminal::Disconnected;
                    break;
                }
            }
        }

        replay.append(&mut self.replay);
        self.replay = replay;
        (Bytes::from(prefix), self, terminal)
    }

    pub(crate) fn tee(mut self) -> (Self, Self) {
        let (left_tx, left_rx) =
            mpsc::sync_channel::<GatewayByteStreamItem>(GATEWAY_STREAM_CHANNEL_CAPACITY);
        let (right_tx, right_rx) =
            mpsc::sync_channel::<GatewayByteStreamItem>(GATEWAY_STREAM_CHANNEL_CAPACITY);
        let consumers = Arc::new(AtomicUsize::new(2));
        let relay_consumers = Arc::clone(&consumers);
        thread::spawn(move || loop {
            if relay_consumers.load(Ordering::Acquire) == 0 {
                return;
            }
            match self.recv_timeout(GATEWAY_STREAM_TEE_POLL_INTERVAL) {
                Ok(item) => {
                    let is_terminal = matches!(
                        item,
                        GatewayByteStreamItem::Eof | GatewayByteStreamItem::Error(_)
                    );
                    let left_open = left_tx.send(item.clone()).is_ok();
                    let right_open = right_tx.send(item).is_ok();
                    if is_terminal || (!left_open && !right_open) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = left_tx.send(GatewayByteStreamItem::Eof);
                    let _ = right_tx.send(GatewayByteStreamItem::Eof);
                    return;
                }
            }
        });
        (
            Self::from_receiver_with_parts(left_rx, None, Some(Arc::clone(&consumers))),
            Self::from_receiver_with_parts(right_rx, None, Some(consumers)),
        )
    }

    pub(crate) fn read_all_bytes(mut self) -> Result<Bytes, String> {
        let mut buffer = Vec::new();
        loop {
            match self.recv() {
                Ok(GatewayByteStreamItem::Chunk(bytes)) => buffer.extend_from_slice(bytes.as_ref()),
                Ok(GatewayByteStreamItem::Eof) => return Ok(Bytes::from(buffer)),
                Ok(GatewayByteStreamItem::Error(err)) => return Err(err),
                Err(_) => return Ok(Bytes::from(buffer)),
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct GatewayStreamResponse {
    status: reqwest::StatusCode,
    headers: reqwest::header::HeaderMap,
    body: GatewayByteStream,
}

impl GatewayStreamResponse {
    pub(crate) fn new(
        status: reqwest::StatusCode,
        headers: reqwest::header::HeaderMap,
        body: GatewayByteStream,
    ) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub(crate) fn from_blocking_response(response: reqwest::blocking::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = GatewayByteStream::from_blocking_response(response);
        Self::new(status, headers, body)
    }

    pub(crate) fn status(&self) -> reqwest::StatusCode {
        self.status
    }

    pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
        &self.headers
    }

    pub(crate) fn read_all_bytes(self) -> Result<Bytes, String> {
        self.body.read_all_bytes()
    }

    pub(crate) fn into_body(self) -> GatewayByteStream {
        self.body
    }

    fn prefetch_until<F>(
        self,
        max_bytes: usize,
        idle_timeout: Option<Duration>,
        wall_clock_timeout: Option<Duration>,
        should_stop: F,
    ) -> (Bytes, Self, GatewayStreamPrefetchTerminal)
    where
        F: Fn(&[u8]) -> bool,
    {
        let Self {
            status,
            headers,
            body,
        } = self;
        let (prefix, body, terminal) =
            body.prefetch_until(max_bytes, idle_timeout, wall_clock_timeout, should_stop);
        (prefix, Self::new(status, headers, body), terminal)
    }
}

impl Drop for GatewayByteStream {
    fn drop(&mut self) {
        if let Some(consumers) = self.tee_consumers.take() {
            let previous = consumers.fetch_sub(1, Ordering::AcqRel);
            debug_assert!(previous > 0, "gateway stream tee consumer count underflow");
        }
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}

#[derive(Debug)]
pub(crate) enum GatewayUpstreamResponse {
    Blocking(reqwest::blocking::Response),
    Stream(GatewayStreamResponse),
}

impl GatewayUpstreamResponse {
    pub(crate) fn status(&self) -> reqwest::StatusCode {
        match self {
            Self::Blocking(response) => response.status(),
            Self::Stream(response) => response.status(),
        }
    }

    pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
        match self {
            Self::Blocking(response) => response.headers(),
            Self::Stream(response) => response.headers(),
        }
    }

    pub(crate) fn into_buffered(self) -> Result<(Bytes, Self), String> {
        let status = self.status();
        let headers = self.headers().clone();
        let body = match self {
            Self::Blocking(response) => response
                .bytes()
                .map_err(|err| format!("read upstream response body failed: {err}"))?,
            Self::Stream(response) => response.read_all_bytes()?,
        };
        let rebuilt = Self::Stream(GatewayStreamResponse::new(
            status,
            headers,
            GatewayByteStream::from_bytes(body.clone()),
        ));
        Ok((body, rebuilt))
    }

    pub(crate) fn prefetch_stream_prefix<F>(
        self,
        max_bytes: usize,
        idle_timeout: Option<Duration>,
        wall_clock_timeout: Option<Duration>,
        should_stop: F,
    ) -> (Bytes, Self, GatewayStreamPrefetchTerminal)
    where
        F: Fn(&[u8]) -> bool,
    {
        let response = match self {
            Self::Blocking(response) => GatewayStreamResponse::from_blocking_response(response),
            Self::Stream(response) => response,
        };
        let (prefix, response, terminal) =
            response.prefetch_until(max_bytes, idle_timeout, wall_clock_timeout, should_stop);
        (prefix, Self::Stream(response), terminal)
    }
}

impl From<reqwest::blocking::Response> for GatewayUpstreamResponse {
    fn from(response: reqwest::blocking::Response) -> Self {
        Self::Blocking(response)
    }
}

#[cfg(test)]
#[path = "response_tests.rs"]
mod tests;
