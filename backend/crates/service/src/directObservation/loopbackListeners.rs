//! 正式运行订阅网关共享入口，原生 socket 在观测运行时重新注册；独立双栈监听仅供隔离测试。
use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr},
};
use tokio::net::{TcpListener, TcpStream};

const bindAttempts: usize = 8;

// 两个 listener 共用一个生命周期和连接配额；任一绑定失败都不发布可用配置，不监听通配地址。
pub(super) struct LoopbackListeners {
    ipv4: Option<TcpListener>,
    ipv6: Option<TcpListener>,
    shared: Option<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<std::net::TcpStream>>>,
    port: u16,
}

impl LoopbackListeners {
    // 正式启动只订阅已发布网关端口；隔离测试显式使用私有双栈监听，绑定失败直接返回。
    pub(super) async fn bind() -> io::Result<Self> {
        if let Some((port, receiver)) = if cfg!(test) {
            None
        } else {
            super::sharedIngress::subscribe()?
        } {
            return Ok(Self {
                ipv4: None,
                ipv6: None,
                shared: Some(tokio::sync::Mutex::new(receiver)),
                port,
            });
        }
        if !cfg!(test) {
            return Err(io::Error::other("代理中转入口尚未绑定"));
        }
        for _ in 0..bindAttempts {
            let ipv4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let port = ipv4.local_addr()?.port();
            match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
                Ok(ipv6) => {
                    return Ok(Self {
                        ipv4: Some(ipv4),
                        ipv6: Some(ipv6),
                        shared: None,
                        port,
                    })
                }
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
        if let Some(receiver) = &self.shared {
            let stream = receiver
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| io::Error::other("共享入口已关闭"))?;
            return TcpStream::from_std(stream);
        }
        tokio::select! {
            accepted = self.ipv4.as_ref().expect("独立测试监听").accept() => accepted.map(|(stream, _)| stream),
            accepted = self.ipv6.as_ref().expect("独立测试监听").accept() => accepted.map(|(stream, _)| stream),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/observation/loopbackListenerTests.rs"]
mod tests;
