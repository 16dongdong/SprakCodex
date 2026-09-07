//! HTTP CONNECT 与 TLS 位于独立监听端口；推理流量只旁路读取 usage，认证端点走不解密隧道。
use super::{
    certificateAuthority::Authority,
    loopbackListeners::LoopbackListeners,
    recordSink::{Exchange, RecordSink},
    streamObserver,
    usageParser::UsageParser,
    websocketRelay,
};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use std::{convert::Infallible, io, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::Semaphore,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tungstenite::proxy::ProxyConfig;

pub(super) type ResponseBody = UnsyncBoxBody<Bytes, io::Error>;
pub(super) const connectTimeout: Duration = Duration::from_secs(20);
const maxConnections: usize = 128;

pub(super) struct Engine {
    pub authority: Authority,
    pub client: reqwest::Client,
    pub websocketTls: Arc<rustls::ClientConfig>,
    pub proxy: Option<ProxyConfig>,
    pub sink: RecordSink,
    pub cancel: CancellationToken,
    pub tasks: TaskTracker,
}

impl Engine {
    // 创建无重试、无重定向的连接池，401/429 等响应只转发一次，不触发刷新或账号替换。
    pub async fn new(
        authority: Authority,
        sink: RecordSink,
        proxy: Option<String>,
        cancel: CancellationToken,
    ) -> Result<Self, String> {
        // 工作区同时编译 ring 与 aws-lc；显式选择 provider，避免 WebSocket 首次连接时全局选择歧义导致 panic。
        // 根证书仍使用公共可信根，绝不把下游临时 CA 加入真实上游的信任集合。
        let websocketTls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| "初始化上游 TLS 版本失败")?
        .with_root_certificates(rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        ))
        .with_no_client_auth();
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(connectTimeout)
            .read_timeout(Duration::from_secs(180))
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd();
        let proxy = match proxy.filter(|value| !value.trim().is_empty()) {
            Some(address) => {
                let parsed = ProxyConfig::parse(&address)
                    .map_err(|_| "观测出口仅支持 HTTP CONNECT 或 SOCKS5 代理")?;
                builder =
                    builder.proxy(reqwest::Proxy::all(&address).map_err(|_| "观测出口地址无效")?);
                Some(parsed)
            }
            None => None,
        };
        let client = builder.build().map_err(|_| "创建观测上游连接池失败")?;
        Ok(Self {
            authority,
            client,
            websocketTls: Arc::new(websocketTls),
            proxy,
            sink,
            cancel,
            tasks: TaskTracker::new(),
        })
    }

    // 建立原目标 TCP 或通过已配置的网络出口建立隧道，绝不读取账号代理或修改上游目标。
    pub async fn connect(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        let connecting = async {
            if let Some(proxy) = &self.proxy {
                let stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
                tokio_tungstenite::proxy::connect_via_proxy(stream, proxy, host, port)
                    .await
                    .map_err(|_| io::Error::other("观测出口隧道失败"))
            } else {
                TcpStream::connect((host, port)).await
            }
        };
        tokio::time::timeout(connectTimeout, connecting)
            .await
            .map_err(|_| io::Error::other("观测出口连接超时"))?
    }
}

// 限制监听连接数；停止先取消网络任务，再给解码和持久化任务完成 EOF 处理的机会。
pub(super) async fn serve(listener: LoopbackListeners, engine: Arc<Engine>) {
    // 续签与连接共用运行期取消和任务回收；不是游离的定时器，监听停止后维护任务必须一起退出。
    engine.tasks.spawn(engine.authority.maintenance(engine.cancel.clone()));
    let permits = Arc::new(Semaphore::new(maxConnections));
    loop {
        let accepted = tokio::select! { _ = engine.cancel.cancelled() => break, accepted = listener.accept() => accepted };
        let Ok(stream) = accepted else {
            engine.cancel.cancel();
            break;
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let connectionEngine = engine.clone();
        engine.tasks.spawn(async move {
            let _permit = permit;
            serveAccepted(stream, connectionEngine).await;
        });
    }
    drop(listener);
    engine.tasks.close();
    if tokio::time::timeout(Duration::from_secs(5), engine.tasks.wait())
        .await
        .is_err()
    {
        log::warn!("观测连接清理超过五秒，运行时将终止剩余任务");
    }
}

// 先可靠读取入口类型，再交给 HTTP 或 Relay；握手探测有总时限且随服务停止取消。
async fn serveAccepted(mut stream: TcpStream, engine: Arc<Engine>) {
    use super::relayIngress::{self, Ingress};
    let detected = tokio::select! {
        _ = engine.cancel.cancelled() => return,
        result = tokio::time::timeout(connectTimeout, relayIngress::readIngress(&mut stream)) => result,
    };
    match detected {
        Ok(Ok(Ingress::Http(prefix))) => {
            serveConnection(relayIngress::replay(stream, prefix), engine, None).await;
        }
        Ok(Ok(Ingress::Relay(target))) => serveRelay(stream, engine, target).await,
        _ => log::debug!("观测入口未收到完整有效的协议前缀"),
    }
}

// 原始 IP 和端口始终保留；白名单以外的域名、非 TLS 协议和非 443 端口全部原样隧道转发。
async fn serveRelay(
    mut stream: TcpStream,
    engine: Arc<Engine>,
    target: cpcommon::hook_proxy::HookProxyTarget,
) {
    use super::relayIngress;
    let operation = async {
        let (host, prefix) = if target.port == 443 {
            tokio::time::timeout(connectTimeout, relayIngress::readHello(&mut stream))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Relay 握手探测超时"))??
        } else {
            (None, Vec::new())
        };
        let config = host
            .as_deref()
            .and_then(|host| engine.authority.hosts.get(host))
            .cloned();
        let mut stream = relayIngress::replay(stream, prefix);
        if let (Some(host), Some(config)) = (host, config) {
            let accepted = tokio::time::timeout(
                connectTimeout,
                tokio_rustls::TlsAcceptor::from(config).accept(stream),
            )
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Relay TLS 握手超时"))??;
            serveConnection(
                accepted,
                engine.clone(),
                Some(format!("{host}:{}", target.port)),
            )
            .await;
        } else {
            let mut upstream = engine.connect(&target.ip.to_string(), target.port).await?;
            tokio::io::copy_bidirectional(&mut stream, &mut upstream).await?;
        }
        Ok::<_, io::Error>(())
    };
    tokio::select! {
        _ = engine.cancel.cancelled() => {},
        result = operation => if result.is_err() {
            // 日志只记录失败阶段，不输出握手字节、凭据或请求正文。
            log::debug!("Relay TLS 或原目标隧道连接结束");
        }
    }
}
// 同一实现承载明文代理与解密后的 HTTP/1.1、HTTP/2；固定 tunnel 防止请求 authority 越界。
pub(super) fn serveConnection<S>(
    stream: S,
    engine: Arc<Engine>,
    tunnel: Option<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    Box::pin(async move {
        let cancel = engine.cancel.clone();
        let service = hyper::service::service_fn(move |request| {
            let engine = engine.clone();
            let tunnel = tunnel.clone();
            async move { Ok::<_, Infallible>(dispatch(request, engine, tunnel).await) }
        });
        let builder = auto::Builder::new(TokioExecutor::new());
        tokio::select! {
            _ = cancel.cancelled() => {},
            result = builder.serve_connection_with_upgrades(TokioIo::new(stream), service) => {
                if result.is_err() { log::debug!("观测客户端连接关闭或协议不匹配"); }
            }
        }
    })
}

// 将 CONNECT 与实际推理请求分离；任何鉴权头只留在转发请求内，不进入诊断信息。
async fn dispatch(
    mut request: Request<Incoming>,
    engine: Arc<Engine>,
    tunnel: Option<String>,
) -> Response<ResponseBody> {
    if request.method() == Method::CONNECT {
        if tunnel.is_some() {
            return reply(StatusCode::BAD_REQUEST, "嵌套隧道无效");
        }
        return connectTunnel(request, engine).await;
    }
    let target = match targetUrl(&request, tunnel.as_deref()) {
        Ok(target) => target,
        Err(_) => return reply(StatusCode::BAD_REQUEST, "观测请求地址无效"),
    };
    let host = target.host_str().unwrap_or("");
    let tracked = engine.authority.hosts.contains_key(host) && inferencePath(target.path());
    if request
        .headers()
        .get(header::UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
    {
        return websocketRelay::upgrade(request, engine, target, tracked).await;
    }
    let mut exchange = Exchange::new(host, target.path(), "http");
    exchange.method = request.method().to_string();
    let headers = request.headers_mut();
    stripHopHeaders(headers);
    let (parts, body) = request.into_parts();
    let outgoing = engine
        .client
        .request(parts.method, target)
        .headers(parts.headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()));
    match outgoing.send().await {
        Ok(response) => {
            let status = response.status();
            let mut headers = response.headers().clone();
            stripHopHeaders(&mut headers);
            let responseBody = if tracked {
                streamObserver::observe(response, engine.clone(), exchange)
            } else {
                http_body_util::StreamBody::new(response.bytes_stream().map(|result| {
                    result
                        .map(hyper::body::Frame::data)
                        .map_err(|_| io::Error::other("上游响应读取失败"))
                }))
                .boxed_unsync()
            };
            let mut response = Response::new(responseBody);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            response
        }
        Err(_) => {
            if tracked {
                let parsed = UsageParser::failure("上游连接或请求发送失败");
                engine.sink.finish(exchange, parsed, 502).await;
            }
            reply(StatusCode::BAD_GATEWAY, "观测上游请求失败")
        }
    }
}

// 白名单站点做 TLS 观测，其余 CONNECT 原样透传，包括 auth.openai.com 的登录和刷新请求。
async fn connectTunnel(
    mut request: Request<Incoming>,
    engine: Arc<Engine>,
) -> Response<ResponseBody> {
    let Some(authority) = request.uri().authority().cloned() else {
        return reply(StatusCode::BAD_REQUEST, "隧道目标缺失");
    };
    let host = authority.host().to_lowercase();
    let port = authority.port_u16().unwrap_or(443);
    let server = if port == 443 {
        engine.authority.hosts.get(&host).cloned()
    } else {
        None
    };
    let upstream = if server.is_none() {
        match engine.connect(&host, port).await {
            Ok(stream) => Some(stream),
            Err(_) => return reply(StatusCode::BAD_GATEWAY, "隧道连接失败"),
        }
    } else {
        None
    };
    let upgraded = hyper::upgrade::on(&mut request);
    let taskEngine = engine.clone();
    engine.tasks.spawn(async move {
        let operation = async {
            let upgraded = upgraded
                .await
                .map_err(|_| io::Error::other("隧道升级失败"))?;
            let mut stream = TokioIo::new(upgraded);
            if let Some(config) = server {
                let accepted = tokio::time::timeout(
                    connectTimeout,
                    tokio_rustls::TlsAcceptor::from(config).accept(stream),
                )
                .await
                .map_err(|_| io::Error::other("观测 TLS 握手超时"))??;
                // 装箱打断 CONNECT -> serveConnection -> dispatch 的递归 future 类型。
                Box::pin(serveConnection(
                    accepted,
                    taskEngine.clone(),
                    Some(authority.to_string()),
                ))
                .await;
            } else if let Some(mut upstream) = upstream {
                tokio::io::copy_bidirectional(&mut stream, &mut upstream).await?;
            }
            Ok::<_, io::Error>(())
        };
        tokio::select! {
            _ = taskEngine.cancel.cancelled() => {},
            result = operation => { if result.is_err() { log::debug!("观测 TLS 或隧道连接结束"); } }
        }
    });
    reply(StatusCode::OK, "")
}

// 隧道内使用原 CONNECT 主机和端口，拒绝不同 authority；查询串用于转发但永不写日志。
fn targetUrl(request: &Request<Incoming>, tunnel: Option<&str>) -> Result<url::Url, ()> {
    let target = if let Some(authority) = tunnel {
        if let Some(actual) = request.uri().authority() {
            let expected: hyper::http::uri::Authority = authority.parse().map_err(|_| ())?;
            if !actual.host().eq_ignore_ascii_case(expected.host())
                || actual.port_u16().unwrap_or(443) != expected.port_u16().unwrap_or(443)
            {
                return Err(());
            }
        }
        format!(
            "https://{authority}{}",
            request
                .uri()
                .path_and_query()
                .map_or("/", |part| part.as_str())
        )
    } else {
        request.uri().to_string()
    };
    let parsed = url::Url::parse(&target).map_err(|_| ())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(());
    }
    Ok(parsed)
}

// 只记录推理操作，排除登录、令牌刷新及其它可能承载秘密的账户接口。
pub(super) fn inferencePath(path: &str) -> bool {
    matches!(
        path,
        "/v1/responses"
            | "/v1/chat/completions"
            | "/backend-api/codex/responses"
            | "/backend-api/codex/responses/compact"
            | "/v1/responses/compact"
    )
}

// RFC 连接级头不得跨代理跳转；业务头保持原值，尤其不替换 Authorization/Cookie。
pub(super) fn stripHopHeaders(headers: &mut header::HeaderMap) {
    let names: Vec<String> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(',').map(|name| name.trim().to_owned()))
        .collect();
    for name in names {
        headers.remove(name);
    }
    for name in [
        "connection",
        "proxy-connection",
        "proxy-authorization",
        "proxy-authenticate",
        "keep-alive",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

// 构造不含上游细节的短诊断响应，避免将带认证参数的 URL 或头写入客户端错误及日志。
pub(super) fn reply(status: StatusCode, message: &'static str) -> Response<ResponseBody> {
    let mut response = Response::new(
        Full::new(Bytes::from_static(message.as_bytes()))
            .map_err(|never| match never {})
            .boxed_unsync(),
    );
    *response.status_mut() = status;
    response
}
