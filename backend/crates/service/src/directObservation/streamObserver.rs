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

const pipeCapacity: usize = 64 * 1024;

// 在响应头到达时挂接旁路，管道写入使用背压而非无限队列；关闭/取消也会让解析器得到 EOF。
pub(super) fn observe(
    response: reqwest::Response,
    engine: Arc<Engine>,
    exchange: Exchange,
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
    let (writer, reader) = tokio::io::duplex(pipeCapacity);
    let taskEngine = engine.clone();
    engine.tasks.spawn(async move {
        let mut parsed = UsageParser::default();
        let decoder = decoder(reader, &encoding);
        match decoder {
            Ok(mut decoded) => {
                let mut chunk = [0u8; 8192];
                loop {
                    match decoded.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(length) => parsed.feed(&chunk[..length], sse),
                        Err(_) => {
                            parsed.problem = Some("观测响应解压或读取失败，用量可能缺失");
                            break;
                        }
                    }
                }
                parsed.finish(sse);
            }
            Err(()) => {
                parsed.problem = Some("观测响应压缩格式尚不支持，用量未知");
            }
        }
        taskEngine.sink.finish(exchange, parsed, status).await;
    });
    let forwarding = futures_util::stream::unfold(
        (Box::pin(response.bytes_stream()), Some(writer)),
        |(mut upstream, mut writer)| async move {
            let result = upstream.next().await?;
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
            Some((frame, (upstream, writer)))
        },
    );
    http_body_util::StreamBody::new(forwarding).boxed_unsync()
}

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
