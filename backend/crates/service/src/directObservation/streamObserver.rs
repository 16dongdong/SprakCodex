//! 原始压缩流直接转发；解压副本经有界管道消费，长 SSE 不依赖固定长度正文预览。
use super::{
    recordSink::Exchange,
    transport::{Engine, ResponseBody},
    usageParser::UsageParser,
};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use std::{io, pin::Pin, sync::Arc};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};

const pipeCapacity: usize = 64 * 1024;
const formatProbeBytes: usize = 512;
const retryProbeBytes: usize = 64 * 1024;
const retryProbeTimeout: Duration = Duration::from_secs(10);

pub(super) enum RetryPreflight {
    Ready {
        response: reqwest::Response,
        prefix: Vec<bytes::Bytes>,
    },
    Quota(String),
}

// 在任何字节交付给 Codex 前识别明确额度终态；出现真实输出后立即提交，禁止跨账号重复执行已开始的工作。
pub(super) async fn preflightRetry(mut response: reqwest::Response) -> RetryPreflight {
    let status = response.status().as_u16();
    let mut prefix = Vec::new();
    let mut joined = Vec::new();
    let probe = async {
        while joined.len() < retryProbeBytes {
            let Ok(Some(chunk)) = response.chunk().await else { break };
            joined.extend_from_slice(&chunk);
            prefix.push(chunk);
            if let Some(diagnostic) = quotaDiagnostic(&joined, status) {
                return Some(diagnostic);
            }
            if hasDeliverableOutput(&joined) {
                break;
            }
        }
        None
    };
    if let Ok(Some(diagnostic)) = timeout(retryProbeTimeout, probe).await {
        return RetryPreflight::Quota(diagnostic);
    }
    RetryPreflight::Ready { response, prefix }
}

fn quotaDiagnostic(body: &[u8], status: u16) -> Option<String> {
    if !(status == 429 || body.windows(17).any(|part| part == b"usage_limit_reach")) {
        return None;
    }
    let text = String::from_utf8_lossy(body);
    for frame in text.replace("\r\n", "\n").split("\n\n") {
        let json = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
            .collect::<String>();
        let candidate = if json.is_empty() { frame.trim() } else { json.as_str() };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
            let error = value.pointer("/response/error").or_else(|| value.get("error"));
            if let Some(error) = error {
                let diagnostic = error.to_string();
                if crate::sessionRouting::quotaCooldown(&diagnostic, codexmanager_core::storage::now_ts()).is_some() {
                    return Some(diagnostic);
                }
            }
            if value.get("type").and_then(serde_json::Value::as_str)
                == Some("response.output_text.delta")
            {
                let delta = value
                    .get("delta")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if delta.contains("usage limit") || delta.contains("quota exceeded") {
                    return Some(
                        serde_json::json!({"code":"usage_limit_reached"}).to_string(),
                    );
                }
            }
        }
    }
    None
}

fn hasDeliverableOutput(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body);
    [
        "response.output_text.delta",
        "response.function_call_arguments.delta",
        "response.reasoning_summary_text.delta",
        "\"output\"",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

// 在响应头到达时挂接旁路，管道写入使用背压而非无限队列；关闭/取消也会让解析器得到 EOF。
pub(super) fn observePrefetched(
    response: reqwest::Response,
    engine: Arc<Engine>,
    mut exchange: Exchange,
    prefix: Vec<bytes::Bytes>,
) -> ResponseBody {
    let sse = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        });
    let encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let status = response.status().as_u16();
    exchange.responseHeaders = super::detailCapture::headers(response.headers());
    exchange.firstResponseMs =
        Some(exchange.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
    let (writer, reader) = tokio::io::duplex(pipeCapacity);
    let taskEngine = engine.clone();
    engine.tasks.spawn(async move {
        let parsed = match decoder(reader, &encoding) {
            Ok(decoded) => consumeDecoded(decoded, sse).await,
            Err(()) => UsageParser::failure("观测响应压缩格式尚不支持，用量未知"),
        };
        taskEngine.sink.finish(exchange, parsed, status).await;
    });
    let forwarding = futures_util::stream::unfold(
        (prefix.into_iter(), Box::pin(response.bytes_stream()), Some(writer)),
        |(mut prefix, mut upstream, mut writer)| async move {
            let result = match prefix.next() {
                Some(bytes) => Ok(bytes),
                None => upstream.next().await?,
            };
            if let Ok(bytes) = &result {
                if let Some(pipe) = writer.as_mut() {
                    if pipe.write_all(bytes).await.is_err() {
                        writer = None;
                    }
                }
            }
            let frame = result
                .map(hyper::body::Frame::data)
                .map_err(|_| io::Error::other("观测上游响应中断"));
            Some((frame, (prefix, upstream, writer)))
        },
    );
    http_body_util::StreamBody::new(forwarding).boxed_unsync()
}

// 部分真实 SSE 响应不声明 text/event-stream；先识别解压后字节的 framing，避免 CLI 成功而用量全为空。
// 探测最多缓存 512 字节，所有预读字节都交给同一解析器；仅旁路解码，不改变客户端响应头或正文。
async fn consumeDecoded(
    mut decoded: Pin<Box<dyn AsyncRead + Send>>,
    declaredSse: bool,
) -> UsageParser {
    let mut parsed = UsageParser::default();
    let mut probe = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut format = None;
    loop {
        let count = match decoded.read(&mut chunk).await {
            Ok(0) => break,
            Ok(count) => count,
            Err(_) => {
                parsed.problem = Some("观测响应解压或读取失败，用量可能缺失");
                break;
            }
        };
        if let Some(sse) = format {
            parsed.feed(&chunk[..count], sse);
            continue;
        }
        let taken = count.min(formatProbeBytes - probe.len());
        probe.extend_from_slice(&chunk[..taken]);
        format = formatFromPrefix(&probe)
            .or_else(|| (probe.len() == formatProbeBytes).then_some(declaredSse));
        if let Some(sse) = format {
            log::debug!(
                "HTTP 观测 framing：SSE={sse}，与 Content-Type 一致={}",
                sse == declaredSse
            );
            let content = probe.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&probe);
            parsed.feed(content, sse);
            parsed.feed(&chunk[taken..count], sse);
            probe.clear();
        }
    }
    let sse = format.unwrap_or(declaredSse);
    if !probe.is_empty() {
        parsed.feed(&probe, sse);
    }
    parsed.finish(sse);
    parsed
}

// 识别 JSON 或 SSE 的结构前缀；半个 UTF-8 BOM/字段名继续等待，不按一次网络 read 的长度定型。
fn formatFromPrefix(prefix: &[u8]) -> Option<bool> {
    let prefix = prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(prefix);
    let prefix = prefix.trim_ascii_start();
    if prefix.starts_with(b"{") || prefix.starts_with(b"[") {
        return Some(false);
    }
    if [b"data:".as_slice(), b"event:", b"id:", b"retry:", b":"]
        .iter()
        .any(|field| prefix.starts_with(field))
    {
        return Some(true);
    }
    None
}

#[cfg(test)]
#[path = "../../tests/observation/streamObserverTests.rs"]
mod tests;

// 只在旁路解压；未知编码显式标记，不改 Accept-Encoding，也不改发给客户端的压缩字节。
fn decoder(
    reader: tokio::io::DuplexStream,
    encoding: &str,
) -> Result<Pin<Box<dyn AsyncRead + Send>>, ()> {
    use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZlibDecoder, ZstdDecoder};
    let reader = BufReader::new(reader);
    Ok(match encoding.trim() {
        "" | "identity" => Box::pin(reader),
        "gzip" => Box::pin(GzipDecoder::new(reader)),
        "deflate" => Box::pin(ZlibDecoder::new(reader)),
        "br" => Box::pin(BrotliDecoder::new(reader)),
        "zstd" => Box::pin(ZstdDecoder::new(reader)),
        _ => return Err(()),
    })
}
