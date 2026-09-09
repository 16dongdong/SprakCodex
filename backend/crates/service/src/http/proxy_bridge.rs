use std::io;
use std::time::Duration;

use axum::Router;

// 分流探测并发且有界，慢速前缀不会阻塞其他网关请求；监听析构时 JoinSet 自动取消未完成探测。
struct SharedListener {
    listener: tokio::net::TcpListener,
    pending: tokio::task::JoinSet<io::Result<Option<tokio::net::TcpStream>>>,
}

impl axum::serve::Listener for SharedListener {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;

    // 普通连接原样进入 Axum，观测连接交给已启用的运行期；单连接探测失败仅关闭该连接。
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            tokio::select! {
                accepted = self.listener.accept(), if self.pending.len() < 256 => {
                    match accepted {
                        Ok((stream, _)) => { self.pending.spawn(crate::directObservation::sharedIngress::route(stream)); }
                        Err(error) => {
                            log::error!("共享入口接收失败：{error}");
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                }
                completed = self.pending.join_next(), if !self.pending.is_empty() => {
                    if let Some(Ok(Ok(Some(stream)))) = completed {
                        if let Ok(address) = stream.peer_addr() { return (stream, address); }
                    }
                }
            }
        }
    }

    // 报告实际监听地址，供 Axum 生命周期和绑定验证使用；读取失败保留 IO 错误。
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

/// 函数 `wait_for_shutdown_signal`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
async fn wait_for_shutdown_signal() {
    while !crate::shutdown_requested() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 函数 `serve_proxy_on_listener`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// - listener: 参数 listener
/// - app: 参数 app
///
/// # 返回
/// 返回函数执行结果
async fn serve_proxy_on_listener(listener: tokio::net::TcpListener, app: Router) -> io::Result<()> {
    crate::directObservation::sharedIngress::publish(listener.local_addr()?.port())?;
    axum::serve(
        SharedListener {
            listener,
            pending: tokio::task::JoinSet::new(),
        },
        app,
    )
    .with_graceful_shutdown(wait_for_shutdown_signal())
    .await
    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
}

/// 函数 `run_proxy_server`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
#[allow(non_snake_case)]
pub(crate) async fn run_proxy_server(addr: &str, app: Router) -> io::Result<()> {
    // 中文注释：localhost 在 Windows 上可能只解析到 IPv6；双栈监听可避免客户端栈选择差异导致的连接失败。
    let normalized = if let Some(port) = addr.trim().strip_prefix("127.0.0.1:") {
        format!("localhost:{port}")
    } else {
        addr.trim().to_owned()
    };
    let addr_trimmed = normalized.as_str();
    let binding = addr_trimmed
        .strip_prefix("localhost:")
        .map(|port| (std::net::Ipv4Addr::LOCALHOST, port))
        .or_else(|| {
            addr_trimmed
                .strip_prefix("0.0.0.0:")
                .map(|port| (std::net::Ipv4Addr::UNSPECIFIED, port))
        });
    if let Some((interface, port)) = binding {
        // 保持既有 IPv4 公开范围，仅补充同端口 IPv6 回环；不把本机观测入口扩展到远端 IPv6。
        // 端口 0 也必须复用同一个内核分配值，任一绑定失败均不发布半可用入口。
        let port = port
            .parse::<u16>()
            .map_err(|_| io::Error::other("监听端口无效"))?;
        let ipv4 = tokio::net::TcpListener::bind((interface, port)).await?;
        let port = ipv4.local_addr()?.port();
        let ipv6 = tokio::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port)).await?;
        let (ipv4Result, ipv6Result) = tokio::join!(
            serve_proxy_on_listener(ipv4, app.clone()),
            serve_proxy_on_listener(ipv6, app)
        );
        return ipv4Result.and(ipv6Result);
    }

    let listener = tokio::net::TcpListener::bind(addr_trimmed).await?;
    serve_proxy_on_listener(listener, app).await
}
