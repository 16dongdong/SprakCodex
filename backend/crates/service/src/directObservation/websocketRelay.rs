//! WebSocket 两端独立协商传输扩展，消息与认证保持不变；复用已有 deflate/分片实现。
use super::{
    recordSink::Exchange,
    transport::{connectTimeout, reply, Engine, ResponseBody},
    usageParser::UsageParser,
    websocketObservation::{handshakeProtocol, Observation},
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{
    tungstenite::{
        self,
        extensions::{compression::deflate::DeflateConfig, ExtensionsConfig},
        protocol::{Role, WebSocketConfig},
    },
    WebSocketStream,
};

const connectionIdleTimeout: Duration = Duration::from_secs(300);

// 先完成真实上游握手，再向客户端返回 101；拒绝、过期认证和限流状态不伪装成握手成功。
pub(super) async fn upgrade(
    mut request: Request<Incoming>,
    engine: Arc<Engine>,
    mut target: url::Url,
    tracked: bool,
) -> Response<ResponseBody> {
    let host = target.host_str().unwrap_or("").to_owned();
    let path = target.path().to_owned();
    let routeDecision = if tracked {
        match crate::sessionRouting::resolveRequest(request.headers()).await {
            Ok(decision) => decision,
            Err(error) => {
                log::warn!("会话分流拒绝 WebSocket 握手：{error}");
                return reply(StatusCode::SERVICE_UNAVAILABLE, "会话分流请求失败");
            }
        }
    } else {
        crate::sessionRouting::RouteDecision::Passthrough {
            reason: "non_inference_request",
            sessionId: None,
            routeSource: None,
        }
    };
    if let Err(error) = crate::sessionRouting::applyDecision(request.headers_mut(), &routeDecision)
    {
        log::warn!("会话分流 WebSocket 身份字段无效：{error}");
        return reply(StatusCode::SERVICE_UNAVAILABLE, "会话分流身份无效");
    }
    let mut handshake = Exchange::new(&host, &path, handshakeProtocol);
    handshake.method = "GET".into();
    handshake.captureHeaders(request.headers());
    handshake.captureRouting(&routeDecision);
    let observationHeaders = request.headers().clone();
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
        let connector = tokio_tungstenite::Connector::Rustls(engine.websocketTls.clone());
        tokio_tungstenite::client_async_tls_with_config(
            outgoing,
            stream,
            Some(config),
            Some(connector),
        )
        .await
    };
    let connected = tokio::time::timeout(connectTimeout, connection).await;
    let (mut upstream, handshakeResponse) = match connected {
        Ok(Ok(result)) => result,
        failure => {
            let mut response = reply(StatusCode::BAD_GATEWAY, "WebSocket 上游握手失败");
            let mut parsed = UsageParser::failure("WebSocket 上游握手失败");
            if let Ok(Err(tungstenite::Error::Http(rejected))) = failure {
                let (parts, body) = rejected.into_parts();
                handshake.responseHeaders = super::detailCapture::headers(&parts.headers);
                if let Some(bytes) = &body {
                    parsed.body.feed(bytes);
                    parsed.json(bytes);
                }
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
                    .finish(handshake, parsed, response.status().as_u16())
                    .await;
            }
            return response;
        }
    };
    let observationResponseHeaders = handshakeResponse.headers().clone();
    let upgraded = hyper::upgrade::on(&mut request);
    let taskEngine = engine.clone();
    engine.tasks.spawn(async move {
        let Ok(Ok(stream)) = tokio::time::timeout(connectTimeout, upgraded).await else {
            if tracked { taskEngine.sink.finish(handshake, UsageParser::failure("WebSocket 客户端握手中断"), 499).await; }
            return;
        };
        // 下游不宣告 deflate，上游由成熟库独立解压；不伪称端到端压缩参数保持不变。
        let mut downstream = WebSocketStream::from_raw_socket(TokioIo::new(stream), Role::Server, None).await;
        let mut observation = Observation::default();
        observation.headers = observationHeaders;
        observation.responseHeaders = observationResponseHeaders;
        observation.routing = Some(handshake.routingSnapshot());
        loop {
            let transfer = tokio::select! {
                _ = taskEngine.cancel.cancelled() => break,
                _ = tokio::time::sleep(connectionIdleTimeout) => break,
                message = upstream.next() => match message {
                    Some(Ok(message)) => {
                        if tracked {
                            match observation.response(&message) {
                                Ok(Some((exchange, parsed, status))) => taskEngine.sink.finish(exchange, parsed, status).await,
                                Ok(None) => {},
                                Err(()) => { taskEngine.sink.counters.errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed); },
                            }
                        }
                        downstream.send(message).await
                    }
                    _ => break,
                },
                message = downstream.next() => match message {
                    Some(Ok(message)) => {
                        if tracked && observation.request(&message, (&host, &path)).is_err() {
                            // 排空或容量拒绝后不再向上游发送未登记请求，保证更新的空闲判定真实。
                            taskEngine.sink.counters.errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            break;
                        }
                        upstream.send(message).await
                    },
                    _ => break,
                },
            };
            if transfer.is_err() { break; }
        }
        for (exchange, mut parsed) in observation.unfinished() {
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
