//! 宿主启用或恢复后，进程内把仍连原本机代理的旧连接交给客户端自行重建；不终止进程或修改认证。
use std::{
    mem::ManuallyDrop,
    net::{Shutdown, SocketAddr, TcpStream},
    os::windows::io::FromRawSocket,
};
use windows::Win32::{
    Foundation::ERROR_NO_MORE_ITEMS,
    Networking::WinSock::SOCKET,
    System::{Diagnostics::ProcessSnapshotting::*, Threading::GetCurrentProcess},
};

struct Snapshot {
    snapshot: HPSS,
    marker: HPSSWALK,
}
impl Drop for Snapshot {
    // 快照与遍历器必须成对释放，失败只记静态诊断，不在析构中中断客户端。
    fn drop(&mut self) {
        unsafe {
            if PssWalkMarkerFree(self.marker) != 0 {
                super::imp::log("释放连接快照遍历器失败");
            }
            if PssFreeSnapshot(GetCurrentProcess(), self.snapshot) != 0 {
                super::imp::log("释放连接快照失败");
            }
        }
    }
}

// 只捕获本进程句柄，不复制地址空间或冻结线程；Windows 返回非零状态时停止本轮接入。
fn snapshot() -> Result<Snapshot, u32> {
    unsafe {
        let mut snapshot = HPSS::default();
        let status = PssCaptureSnapshot(GetCurrentProcess(), PSS_CAPTURE_HANDLES, 0, &mut snapshot);
        if status != 0 {
            return Err(status);
        }
        let mut marker = HPSSWALK::default();
        let status = PssWalkMarkerCreate(None, &mut marker);
        if status != 0 {
            PssFreeSnapshot(GetCurrentProcess(), snapshot);
            return Err(status);
        }
        Ok(Snapshot { snapshot, marker })
    }
}

// 宿主先发布 Relay 快照再部署映像，因此初始化线程可同步处理一次旧连接；不创建缺少静态 TLS 的游离线程。
pub(super) fn reconnectOriginalProxy() {
    let Some(config) = super::imp::proxyRuntimeConfig() else {
        return;
    };
    match reconnectWhere(|peer| {
        peer.port() != config.relayPort && super::proxyDiscovery::isPlainHttpProxy(peer)
    }) {
        Ok(count) if count != 0 => {
            super::imp::log(&format!("已请求 {count} 条旧代理连接重新接入观测"))
        }
        Ok(_) => {}
        Err(code) => super::imp::log(&format!("枚举本进程旧连接失败：{code}")),
    }
}

// 通过 Winsock 的 try_clone 固定连接对象，避免句柄复用导致关闭另一条连接；只 shutdown 克隆的同一连接。
// 原始句柄始终由客户端持有，IPC、已接入 Relay、未知代理和非 TCP 句柄均保持不变。
fn reconnectWhere(select: impl Fn(SocketAddr) -> bool) -> Result<usize, u32> {
    let snapshot = snapshot()?;
    let mut count = 0;
    loop {
        let mut entry = PSS_HANDLE_ENTRY::default();
        let status = unsafe {
            PssWalkSnapshot(
                snapshot.snapshot,
                PSS_WALK_HANDLES,
                snapshot.marker,
                Some(std::slice::from_raw_parts_mut(
                    (&mut entry as *mut PSS_HANDLE_ENTRY).cast::<u8>(),
                    std::mem::size_of::<PSS_HANDLE_ENTRY>(),
                )),
            )
        };
        if status == ERROR_NO_MORE_ITEMS.0 {
            return Ok(count);
        }
        if status != 0 {
            return Err(status);
        }
        let socket = SOCKET(entry.Handle.0 as usize);
        if !unsafe { super::imp::socket_is_tcp(socket) } {
            continue;
        }
        let borrowed = ManuallyDrop::new(unsafe { TcpStream::from_raw_socket(socket.0 as _) });
        let Ok(connection) = borrowed.try_clone() else {
            continue;
        };
        let Ok(peer) = connection.peer_addr() else {
            continue;
        };
        if select(peer) && connection.shutdown(Shutdown::Both).is_ok() {
            count += 1;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/warmConnectionsTests.rs"]
mod tests;
