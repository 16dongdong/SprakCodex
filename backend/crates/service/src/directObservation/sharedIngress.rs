//! 网关持有唯一公开监听端口；原生路由头和本机 CONNECT 连接交给观测运行期，其余连接保持原 HTTP 路由。
use std::{
    io,
    sync::{Mutex, OnceLock},
};
use tokio::{net::TcpStream, sync::mpsc};

#[derive(Default)]
struct Registration {
    port: u16,
    receiver: Option<mpsc::Sender<std::net::TcpStream>>,
}
static registration: OnceLock<Mutex<Registration>> = OnceLock::new();

// 监听绑定成功后发布实际端口；同一服务的 IPv4/IPv6 监听允许重复发布，异端口冲突直接报错。
pub(crate) fn publish(port: u16) -> io::Result<()> {
    let mut current = registration
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("入口状态锁损坏"))?;
    if current.port != 0
        && current.port != port
        && current
            .receiver
            .as_ref()
            .is_some_and(|sender| !sender.is_closed())
    {
        return Err(io::Error::other("观测运行中，入口端口尚未释放"));
    }
    current.port = port;
    Ok(())
}

// 观测启动订阅已绑定入口；跨运行时传递标准 socket，在消费者运行时重新注册，不创建第二个 TCP 监听。
pub(super) fn subscribe() -> io::Result<Option<(u16, mpsc::Receiver<std::net::TcpStream>)>> {
    let mut current = registration
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("入口状态锁损坏"))?;
    if current.port == 0 {
        return Ok(None);
    }
    if current
        .receiver
        .as_ref()
        .is_some_and(|sender| !sender.is_closed())
    {
        return Err(io::Error::other("观测入口已被订阅"));
    }
    let (sender, receiver) = mpsc::channel(256);
    current.receiver = Some(sender);
    Ok(Some((current.port, receiver)))
}

// 只对回环连接启用观测分流，防止远端把私有头当成无认证转发入口；peek 不消耗 HTTP 或路由头。
pub(crate) async fn route(stream: TcpStream) -> io::Result<Option<TcpStream>> {
    if !stream.peer_addr()?.ip().is_loopback() {
        return Ok(Some(stream));
    }
    let mut prefix = [0u8; 8];
    let inspect = async {
        loop {
            let count = stream.peek(&mut prefix).await?;
            if count == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            let bytes = &prefix[..count];
            let native = b"CPROXYH1".starts_with(bytes);
            let connect = b"CONNECT ".starts_with(bytes);
            if !native && !connect {
                return Ok(false);
            }
            if count == prefix.len() {
                return Ok(true);
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    };
    let observed = tokio::time::timeout(std::time::Duration::from_secs(5), inspect)
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
    if !observed {
        return Ok(Some(stream));
    }
    let sender = registration
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("入口状态锁损坏"))?
        .receiver
        .clone();
    let sender = sender.ok_or_else(|| io::Error::other("直连观测未启用"))?;
    sender
        .try_send(stream.into_std()?)
        .map_err(|_| io::Error::other("观测入口已关闭或连接队列已满"))?;
    Ok(None)
}

#[cfg(test)]
#[path = "../../tests/observation/sharedIngressTests.rs"]
mod tests;
