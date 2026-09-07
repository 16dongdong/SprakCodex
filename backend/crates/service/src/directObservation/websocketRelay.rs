//! WebSocket 两端独立协商传输扩展，消息与认证保持不变；复用已有 deflate/分片实现。
use super::{
    recordSink::Exchange,
    transport::{connectTimeout, reply, Engine, ResponseBody},
    usageParser::{maxEventBytes, UsageParser},
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio_tungstenite::{
    tungstenite::{
        self,
        extensions::{compression::deflate::DeflateConfig, ExtensionsConfig},
        protocol::{Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};

const maxPendingResponses: usize = 64;

// 先完成真实上游握手，再向客户端返回 101；拒绝、过期认证和限流状态不伪装成握手成功。
pub(super) async fn upgrade(
    mut request: Request<Incoming>,
    engine: Arc<Engine>,
    mut target: url::Url,
    tracked: bool,
) -> Response<ResponseBody> {
    let host = target.host_str().unwrap_or("").to_owned();
    let path = target.path().to_owned();
    let handshake = Exchange::new(&host, &path, "websocket");
    let scheme = if target.scheme() == "https" {
        "wss"
    } else {
        "ws"
    };
    if target.set_scheme(scheme).is_err() {
        return reply(StatusCode::BAD_REQUEST, "WebSocket 地址无效");
    }
    let mut outgoing = Request::new(());
    *outgoing.uri_mut() = match target.as_str().parse() {
        Ok(uri) => uri,
        Err(_) => return reply(StatusCode::BAD_REQUEST, "WebSocket 地址无效"),
    };
    *outgoing.headers_mut() = request.headers().clone();
    if tungstenite::handshake::server::create_response(&outgoing).is_err() {
        return reply(StatusCode::BAD_REQUEST, "WebSocket 握手字段无效");
    }
    for name in [
        "proxy-authorization",
        "proxy-connection",
        "sec-websocket-extensions",
    ] {
        outgoing.headers_mut().remove(name);
    }
    let mut config = WebSocketConfig::default();
    let mut extensions = ExtensionsConfig::default();
    extensions.permessage_deflate = Some(DeflateConfig::default());
    config.extensions = extensions;
    let connection = async {
        let stream = engine
            .connect(&host, target.port_or_known_default().unwrap_or(443))
            .await
            .map_err(tungstenite::Error::Io)?;
        tokio_tungstenite::client_async_tls_with_config(outgoing, stream, Some(config), None).await
    };
    let connected = tokio::time::timeout(connectTimeout, connection).await;
    let (mut upstream, handshakeResponse) = match connected {
        Ok(Ok(result)) => result,
        failure => {
            let mut response = reply(StatusCode::BAD_GATEWAY, "WebSocket 上游握手失败");
            if let Ok(Err(tungstenite::Error::Http(rejected))) = failure {
                let (parts, body) = rejected.into_parts();
                response = Response::from_parts(
                    parts,
                    Full::new(Bytes::from(body.unwrap_or_default()))
                        .map_err(|never| match never {})
                        .boxed_unsync(),
                );
                super::transport::stripHopHeaders(response.headers_mut());
            }
            if tracked {
                engine
                    .sink
                    .finish(
                        handshake,
                        UsageParser::failure("WebSocket 上游握手失败"),
                        response.status().as_u16(),
                    )
                    .await;
            }
            return response;
        }
    };
    let upgraded = hyper::upgrade::on(&mut request);
    let taskEngine = engine.clone();
    engine.tasks.spawn(async move {
        let Ok(Ok(stream)) = tokio::time::timeout(connectTimeout, upgraded).await else {
            if tracked { taskEngine.sink.finish(handshake, UsageParser::failure("WebSocket 客户端握手中断"), 499).await; }
            return;
        };
        // 下游不宣告 deflate，上游由成熟库独立解压；不伪称端到端压缩参数保持不变。
        let mut downstream = WebSocketStream::from_raw_socket(TokioIo::new(stream), Role::Server, None).await;
        let mut pending = HashMap::<String, (Exchange, UsageParser)>::new();
        loop {
            let transfer = tokio::select! {
                _ = taskEngine.cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(300)) => break,
                message = upstream.next() => match message {
                    Some(Ok(message)) => {
                        if tracked { observeMessage(&message, &mut pending, &taskEngine, (&host, &path)).await; }
                        downstream.send(message).await
                    }
                    _ => break,
                },
                message = downstream.next() => match message {
                    Some(Ok(message)) => upstream.send(message).await,
                    _ => break,
                },
            };
            if transfer.is_err() { break; }
        }
        for (_, (exchange, mut parsed)) in pending {
            parsed.problem = Some("WebSocket 在响应完成之前关闭，用量可能缺失");
            taskEngine.sink.finish(exchange, parsed, 499).await;
        }
    });
    let (mut parts, _) = handshakeResponse.into_parts();
    parts.headers.remove("sec-websocket-extensions");
    Response::from_parts(
        parts,
        Full::new(Bytes::new())
            .map_err(|never| match never {})
            .boxed_unsync(),
    )
}

// 连接可包含多个响应；按服务端 response.id 区分，终结事件只提交一次，重放由数据库去重。
// 超大消息仍由网络库原样转发，旁路不再分配 JSON 树；无响应标识的消息不臆造归属。
async fn observeMessage(
    message: &Message,
    pending: &mut HashMap<String, (Exchange, UsageParser)>,
    engine: &Engine,
    target: (&str, &str),
) {
    let bytes: &[u8] = match message {
        Message::Text(text) => text.as_bytes(),
        Message::Binary(bytes) => bytes,
        _ => return,
    };
    if bytes.len() > maxEventBytes {
        return;
    }
    let Ok(event) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };
    let Some(response) = event.get("response") else {
        return;
    };
    let Some(identity) = response
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| value.len() <= 256)
    else {
        return;
    };
    if !pending.contains_key(identity) && pending.len() >= maxPendingResponses {
        engine
            .sink
            .counters
            .errors
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let (_, parsed) = pending.entry(identity.into()).or_insert_with(|| {
        (
            Exchange::new(target.0, target.1, "websocket"),
            UsageParser::default(),
        )
    });
    parsed.message(&event);
    if parsed.terminal {
        if let Some((exchange, parsed)) = pending.remove(identity) {
            let status = if parsed.problem.is_some() { 502 } else { 200 };
            engine.sink.finish(exchange, parsed, status).await;
        }
    }
}
