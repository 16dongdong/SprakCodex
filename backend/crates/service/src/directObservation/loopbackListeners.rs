//! 原生 socket 保留地址族，因此同一个 Relay 端口必须同时接受 IPv4 与 IPv6 回环连接。
use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr},
};
use tokio::net::{TcpListener, TcpStream};

const bindAttempts: usize = 8;

// 两个 listener 共用一个生命周期和连接配额；任一绑定失败都不发布可用配置，不监听通配地址。
pub(super) struct LoopbackListeners {
    ipv4: TcpListener,
    ipv6: TcpListener,
    port: u16,
}

impl LoopbackListeners {
    // 内核先选 IPv4 空闲端口，再绑定同号 IPv6 端口；仅地址冲突重选，其他错误直接向启动方返回。
    pub(super) async fn bind() -> io::Result<Self> {
        for _ in 0..bindAttempts {
            let ipv4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let port = ipv4.local_addr()?.port();
            match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
                Ok(ipv6) => return Ok(Self { ipv4, ipv6, port }),
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "IPv4/IPv6 回环端口持续冲突",
        ))
    }

    // 只在两个地址族都完成绑定后公开端口，调用方以此原子发布 Relay 配置。
    pub(super) fn port(&self) -> u16 {
        self.port
    }

    // Tokio accept 可取消，交替接收两个地址族；上层仅维护一个信号量和任务组，停止时一起回收。
    pub(super) async fn accept(&self) -> io::Result<TcpStream> {
        tokio::select! {
            accepted = self.ipv4.accept() => accepted.map(|(stream, _)| stream),
            accepted = self.ipv6.accept() => accepted.map(|(stream, _)| stream),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/observation/loopbackListenerTests.rs"]
mod tests;
