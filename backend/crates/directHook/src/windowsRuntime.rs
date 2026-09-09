//! Windows 网络回调与运行期配置；不修改原登录、时区、系统代理和子进程行为。
use super::relayControl::RelayControl;
use cpcommon::hook_proxy::{encodeRoute, HookProxyTarget, RouteKind};
use cpcommon::relayContract::RelayConfig;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use windows::core::{s, w, PCSTR, PCWSTR, PSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HMODULE, TRUE};
use windows::Win32::Networking::WinSock::{
    getsockopt, WSAGetLastError, WSASetLastError, AF_INET, AF_INET6, SOCKADDR, SOCKET, SOCK_STREAM,
    SOL_SOCKET, SO_TYPE, WSABUF,
};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
static NETWORK_READY: AtomicBool = AtomicBool::new(false);
static RELAY_CONTROL: OnceLock<Mutex<RelayControl>> = OnceLock::new();
static READY_EVENT: AtomicIsize = AtomicIsize::new(0);
static LOADED_EVENT: AtomicIsize = AtomicIsize::new(0);
const SIO_GET_EXTENSION_FUNCTION_POINTER: u32 = 0xC8000006;
const WSAID_CONNECTEX: windows::core::GUID =
    windows::core::GUID::from_u128(0x25a207b9_ddf3_4660_8ee9_76e58c74063e);
const WSAEINPROGRESS: i32 = 10036;
const WSAEOPNOTSUPP: i32 = 10045;
const WSAEWOULDBLOCK: i32 = 10035;
const WSAECONNRESET: i32 = 10054;
const WSAEAFNOSUPPORT: i32 = 10047;
const WSA_IO_PENDING: i32 = 997;

// 全部 trampoline 就绪后才读取当前运行实例；配置与内核线程寿命共同决定是否改连。
pub(super) fn proxyRuntimeConfig() -> Option<Arc<RelayConfig>> {
    if !NETWORK_READY.load(Ordering::Acquire) {
        return None;
    }
    let settings = relaySnapshot()?;
    if settings
        .caCertificatePath
        .as_deref()
        .is_some_and(|path| !super::trustProvider::providedFor(path))
    {
        return None;
    }
    Some(settings)
}

// TLS 读取入口与网络入口共用同一份活跃实例快照；证书初始化可先于 NETWORK_READY 完成。
pub(super) fn relaySnapshot() -> Option<Arc<RelayConfig>> {
    RELAY_CONTROL
        .get_or_init(|| Mutex::new(RelayControl::default()))
        .lock()
        .ok()?
        .read()
}

// 内存映像没有磁盘目录；诊断只发往调试输出，不在目标或安装目录创建文件。
pub(super) fn log(msg: &str) {
    let line: Vec<u16> = format!("[观测模块 pid={}] {msg}\n\0", std::process::id())
        .encode_utf16()
        .collect();
    unsafe { OutputDebugStringW(PCWSTR(line.as_ptr())) };
}

// 初始化完成后发布与本模块版本匹配的命名事件，句柄保留到显式卸载，供宿主后续扫描打开。
fn signal_ready() {
    let name = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(
        std::process::id(),
        cpcommon::relayContract::deploymentIdentity,
    ));
    unsafe {
        if READY_EVENT.load(Ordering::SeqCst) != 0 {
            return;
        }
        match CreateEventW(None, true, false, &name) {
            Ok(event) => {
                match SetEvent(event) {
                    Ok(()) => log("cphook ready"),
                    Err(e) => log(&format!("cphook ready 置位失败: {e}")),
                }
                READY_EVENT.store(event.0 as isize, Ordering::SeqCst);
            }
            Err(e) => log(&format!("cphook ready 事件不存在: {e}")),
        }
    }
}

// 初始化线程创建成功后发布加载事件；宿主用它阻止同一版本内存映像被重复映射，卸载时关闭。
fn signal_loaded() {
    let name = windows::core::HSTRING::from(cpcommon::hook_ready::loaded_event_name(
        std::process::id(),
        cpcommon::relayContract::deploymentIdentity,
    ));
    unsafe {
        if LOADED_EVENT.load(Ordering::SeqCst) != 0 {
            return;
        }
        if let Ok(event) = CreateEventW(None, true, true, &name) {
            LOADED_EVENT.store(event.0 as isize, Ordering::SeqCst);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrIn {
    family: u16,
    port: u16,
    addr: [u8; 4],
    zero: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrIn6 {
    family: u16,
    port: u16,
    flow_info: u32,
    addr: [u8; 16],
    scope_id: u32,
}

#[derive(Clone, Copy)]
enum RelaySockaddr {
    V4(SockAddrIn),
    V6(SockAddrIn6),
}

impl RelaySockaddr {
    fn as_ptr(&self) -> *const SOCKADDR {
        match self {
            RelaySockaddr::V4(addr) => (addr as *const SockAddrIn).cast(),
            RelaySockaddr::V6(addr) => (addr as *const SockAddrIn6).cast(),
        }
    }

    fn len(&self) -> i32 {
        match self {
            RelaySockaddr::V4(_) => std::mem::size_of::<SockAddrIn>() as i32,
            RelaySockaddr::V6(_) => std::mem::size_of::<SockAddrIn6>() as i32,
        }
    }
}

#[derive(Clone, Copy)]
struct ProxySocketState {
    target: HookProxyTarget,
    viaProxy: bool,
    header_sent: bool,
    failed: bool,
}

type SharedSocketState = Arc<Mutex<ProxySocketState>>;
static PROXY_SOCKETS: OnceLock<Mutex<BTreeMap<usize, SharedSocketState>>> = OnceLock::new();

// 有序表不触发线程随机种子，并只用于定位连接状态；实际头部写入使用每连接锁，避免慢连接阻塞全局。
fn proxy_sockets() -> &'static Mutex<BTreeMap<usize, SharedSocketState>> {
    PROXY_SOCKETS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

// 原生 SOCKET 按当前连接生命周期作为表键，关闭后清除，不把数字句柄当跨连接身份。
fn socket_key(socket: SOCKET) -> usize {
    socket.0
}

// 校验调用方 sockaddr 的族与长度后读取原目标；无效地址返回 None，保留给 Winsock 原入口处理。
unsafe fn sockaddr_target(addr: *const SOCKADDR, len: i32) -> Option<HookProxyTarget> {
    if addr.is_null() || len < 4 {
        return None;
    }
    let family = (*addr).sa_family;
    let (ip, port) = if family == AF_INET && len >= std::mem::size_of::<SockAddrIn>() as i32 {
        let raw = &*(addr as *const SockAddrIn);
        (IpAddr::V4(Ipv4Addr::from(raw.addr)), u16::from_be(raw.port))
    } else if family == AF_INET6 && len >= std::mem::size_of::<SockAddrIn6>() as i32 {
        let raw = &*(addr as *const SockAddrIn6);
        (IpAddr::V6(Ipv6Addr::from(raw.addr)), u16::from_be(raw.port))
    } else {
        return None;
    };
    if port == 0 {
        return None;
    }
    Some(HookProxyTarget {
        ip,
        port,
        pid: std::process::id(),
    })
}

// 本地 IPC 不参与外部 TCP 改连，IPv4 和 IPv6 使用标准库的回环语义。
fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

// 按原 socket 地址族构造相同协议族的 Relay 地址；不识别的族返回 None，避免错误转换内存布局。
unsafe fn relay_sockaddr_for(
    name: *const SOCKADDR,
    name_len: i32,
    relay_port: u16,
) -> Option<RelaySockaddr> {
    if name.is_null() || name_len < 4 {
        return None;
    }
    let family = (*name).sa_family;
    if family == AF_INET {
        return Some(RelaySockaddr::V4(SockAddrIn {
            family: AF_INET.0,
            port: relay_port.to_be(),
            addr: [127, 0, 0, 1],
            zero: [0; 8],
        }));
    }
    if family == AF_INET6 {
        return Some(RelaySockaddr::V6(SockAddrIn6 {
            family: AF_INET6.0,
            port: relay_port.to_be(),
            flow_info: 0,
            addr: Ipv6Addr::LOCALHOST.octets(),
            scope_id: 0,
        }));
    }
    None
}

// 在连接入口读取 socket 类型，只注册 SOCK_STREAM；查询失败不把未知 socket 当成 TCP。
pub(super) unsafe fn socket_is_tcp(socket: SOCKET) -> bool {
    let mut socket_type = 0_i32;
    let mut socket_type_len = std::mem::size_of::<i32>() as i32;
    let ret = getsockopt(
        socket,
        SOL_SOCKET,
        SO_TYPE,
        PSTR((&mut socket_type as *mut i32).cast()),
        &mut socket_type_len,
    );
    ret == 0 && socket_type == SOCK_STREAM.0
}

// 用未改写的 send trampoline 发送协议字节，处理短写并限制 WouldBlock 等待；失败返回 false。
unsafe fn send_header_bytes(socket: SOCKET, bytes: &[u8]) -> bool {
    let send: SendFn = original(&SEND, "SEND");
    let mut sent = 0usize;
    // 私有头很小(32 字节)且 relay 在本机,发送缓冲几乎总能立即容纳。但非阻塞 socket
    // (Chromium/Node 常用)偶发 WSAEWOULDBLOCK 时不能当致命错误直接断连,否则会触发
    // “网络错误”。这里对 WouldBlock 做有界自旋等待,确保头在任何应用数据之前完整送出。
    let mut spins = 0u32;
    while sent < bytes.len() {
        let remaining = bytes.len() - sent;
        let chunk_len = remaining.min(i32::MAX as usize) as i32;
        let ret = send(socket, bytes[sent..].as_ptr().cast(), chunk_len, 0);
        if ret > 0 {
            sent += ret as usize;
            continue;
        }
        let err = WSAGetLastError().0;
        if err == WSAEWOULDBLOCK && spins < 2000 {
            spins += 1;
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        return false;
    }
    true
}

// 编码共享的固定头并发送，失败设连接重置错误，不继续发送应用正文。
unsafe fn send_proxy_header(socket: SOCKET, state: ProxySocketState) -> bool {
    let header = encodeRoute(
        &state.target,
        if state.viaProxy {
            RouteKind::HttpProxy
        } else {
            RouteKind::Direct
        },
    );
    if send_header_bytes(socket, &header) {
        true
    } else {
        WSASetLastError(WSAECONNRESET);
        false
    }
}

// 私有头和“已发送”标志必须在同一连接锁内提交，避免并发 send/WSASend 重复发送头导致流错位。
unsafe fn ensure_proxy_header_sent(socket: SOCKET) -> bool {
    let key = socket_key(socket);
    let state = {
        let Ok(sockets) = proxy_sockets().lock() else {
            WSASetLastError(WSAECONNRESET);
            return false;
        };
        let Some(state) = sockets.get(&key) else {
            return true;
        };
        state.clone()
    };
    let Ok(mut state) = state.lock() else {
        WSASetLastError(WSAECONNRESET);
        return false;
    };
    if state.failed {
        WSASetLastError(WSAECONNRESET);
        return false;
    }
    if state.header_sent {
        return true;
    }
    if !send_proxy_header(socket, *state) {
        // 半个头已经发送时不能删除状态并让后续 payload 当正常连接穿过；保持失败直到 closesocket 清理。
        state.failed = true;
        return false;
    }
    state.header_sent = true;
    true
}

// 新连接独占其头部状态；closesocket 后相同数字句柄可以安全注册为另一条连接。
fn remember_proxy_socket(socket: SOCKET, target: HookProxyTarget, viaProxy: bool) {
    if let Ok(mut sockets) = proxy_sockets().lock() {
        sockets.insert(
            socket_key(socket),
            Arc::new(Mutex::new(ProxySocketState {
                target,
                viaProxy,
                header_sent: false,
                failed: false,
            })),
        );
    }
}

// 调用方关闭连接时移除表项；正在发送的线程仍持有 Arc，不会读取已释放状态。
fn forget_proxy_socket(socket: SOCKET) {
    if let Ok(mut sockets) = proxy_sockets().lock() {
        sockets.remove(&socket_key(socket));
    }
}

// 连接回调保留原目标供 Relay 使用；关闭状态或不适用目标返回 None，让调用方执行原 Winsock 调用。
unsafe fn proxy_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
    original_connect: impl FnOnce(*const SOCKADDR, i32) -> i32,
) -> Option<i32> {
    let runtime = proxyRuntimeConfig()?;
    if !socket_is_tcp(socket) {
        return None;
    }
    let target = sockaddr_target(name, name_len)?;
    // 回环只匹配目标自己的明文 HTTP 代理端点，CONNECT 由应用原样发送；普通 IPC 和本地服务保持原连接。
    let passthrough = if is_loopback_ip(target.ip) {
        if super::proxyDiscovery::isPlainHttpProxy(std::net::SocketAddr::new(
            target.ip,
            target.port,
        )) {
            true
        } else {
            return None;
        }
    } else {
        false
    };
    let Some(relay) = relay_sockaddr_for(name, name_len, runtime.relayPort) else {
        WSASetLastError(WSAEAFNOSUPPORT);
        return Some(-1);
    };
    let ret = original_connect(relay.as_ptr(), relay.len());
    if ret == 0 {
        // 两种来源都先发送路由头；应用自己的 CONNECT 随后原样发送，宿主据头部保留原代理出口。
        remember_proxy_socket(socket, target, passthrough);
        if !ensure_proxy_header_sent(socket) {
            return Some(-1);
        }
        log(&format!(
            "TCP 已接入观测 Relay, target={}:{} relay_port={} passthrough={}",
            target.ip, target.port, runtime.relayPort, passthrough
        ));
        return Some(0);
    }
    let err = WSAGetLastError().0;
    if err == WSAEWOULDBLOCK || err == WSAEINPROGRESS {
        remember_proxy_socket(socket, target, passthrough);
    }
    WSASetLastError(err);
    Some(ret)
}

type ConnectFn = unsafe extern "system" fn(SOCKET, *const SOCKADDR, i32) -> i32;
type SendFn = unsafe extern "system" fn(SOCKET, *const c_void, i32, i32) -> i32;
type SendToFn =
    unsafe extern "system" fn(SOCKET, *const c_void, i32, i32, *const SOCKADDR, i32) -> i32;
type WsaSendFn = unsafe extern "system" fn(
    SOCKET,
    *const WSABUF,
    u32,
    *mut u32,
    u32,
    *mut c_void,
    *mut c_void,
) -> i32;
type WsaSendToFn = unsafe extern "system" fn(
    SOCKET,
    *const WSABUF,
    u32,
    *mut u32,
    u32,
    *const SOCKADDR,
    i32,
    *mut c_void,
    *mut c_void,
) -> i32;
type WsaConnectFn = unsafe extern "system" fn(
    SOCKET,
    *const SOCKADDR,
    i32,
    *const c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
) -> i32;
type ConnectExFn = unsafe extern "system" fn(
    SOCKET,
    *const SOCKADDR,
    i32,
    *const c_void,
    u32,
    *mut u32,
    *mut c_void,
) -> BOOL;
type CloseSocketFn = unsafe extern "system" fn(SOCKET) -> i32;
type ShutdownFn = unsafe extern "system" fn(SOCKET, i32) -> i32;
static CONNECT: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static SEND: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static SEND_TO: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static WSA_SEND: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static WSA_SEND_TO: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static WSA_CONNECT: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static CONNECT_EX: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static CLOSE_SOCKET: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static SHUTDOWN: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();

// RawDetour 公开的 trampoline 只在槽发布后读取；调用方类型必须与对应 Winsock ABI 完全一致。
unsafe fn original<F: Copy>(slot: &super::hookInstall::DetourSlot, _label: &str) -> F {
    slot.original()
}
// connect 回调仅处理已启用的 TCP 目标；未接管连接及错误返回沿用 Winsock ABI。
unsafe extern "system" fn hook_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let originalConnect: ConnectFn = original(&CONNECT, "CONNECT");
    if activity.isUnloading() {
        return originalConnect(socket, name, name_len);
    }
    if let Some(ret) = proxy_connect(socket, name, name_len, |relay, relay_len| {
        originalConnect(socket, relay, relay_len)
    }) {
        return ret;
    }
    originalConnect(socket, name, name_len)
}

// send 回调先提交一次性 Relay 头，再调用原发送函数；头失败时返回 SOCKET_ERROR。
unsafe extern "system" fn hook_send(
    socket: SOCKET,
    buffer: *const c_void,
    len: i32,
    flags: i32,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let send: SendFn = original(&SEND, "SEND");
    if activity.isUnloading() {
        return send(socket, buffer, len, flags);
    }
    // 只为已注册的 TCP 连接补发私有头，UDP 和其他未接管连接保留原始发送行为。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    send(socket, buffer, len, flags)
}

// sendto 同时服务于 UDP 与 TCP；只有已登记的连接需要私有头，其他调用参数原样转交。
unsafe extern "system" fn hook_send_to(
    socket: SOCKET,
    buffer: *const c_void,
    len: i32,
    flags: i32,
    to: *const SOCKADDR,
    to_len: i32,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let sendTo: SendToFn = original(&SEND_TO, "SEND_TO");
    if activity.isUnloading() {
        return sendTo(socket, buffer, len, flags, to, to_len);
    }
    // Winsock 也允许 TCP 通过 sendto 发送；与 send 共用一次性私有头，未注册的 UDP 保持原样。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    sendTo(socket, buffer, len, flags, to, to_len)
}

// 保留 WSASend 的缓冲区、异步回调与完成语义，只在原调用之前提交已登记连接的私有头。
unsafe extern "system" fn hook_wsa_send(
    socket: SOCKET,
    buffers: *const WSABUF,
    buffer_count: u32,
    bytes_sent: *mut u32,
    flags: u32,
    overlapped: *mut c_void,
    completion: *mut c_void,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let send: WsaSendFn = original(&WSA_SEND, "WSA_SEND");
    if activity.isUnloading() {
        return send(
            socket,
            buffers,
            buffer_count,
            bytes_sent,
            flags,
            overlapped,
            completion,
        );
    }
    // 同 hook_send:UDP 不在此拦截,只负责首个 TCP 应用数据前补发私有头。
    if !ensure_proxy_header_sent(socket) {
        if !bytes_sent.is_null() {
            *bytes_sent = 0;
        }
        return -1;
    }
    send(
        socket,
        buffers,
        buffer_count,
        bytes_sent,
        flags,
        overlapped,
        completion,
    )
}

// 保留目标地址与完成回调，注册的 TCP 连接与其他发送入口共享头部状态。
unsafe extern "system" fn hook_wsa_send_to(
    socket: SOCKET,
    buffers: *const WSABUF,
    buffer_count: u32,
    bytes_sent: *mut u32,
    flags: u32,
    to: *const SOCKADDR,
    to_len: i32,
    overlapped: *mut c_void,
    completion: *mut c_void,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let sendTo: WsaSendToFn = original(&WSA_SEND_TO, "WSA_SEND_TO");
    if activity.isUnloading() {
        return sendTo(
            socket,
            buffers,
            buffer_count,
            bytes_sent,
            flags,
            to,
            to_len,
            overlapped,
            completion,
        );
    }
    // 注册 TCP 连接同样先发送私有头，不修改 UDP、DNS 或应用自己的代理选择。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    sendTo(
        socket,
        buffers,
        buffer_count,
        bytes_sent,
        flags,
        to,
        to_len,
        overlapped,
        completion,
    )
}

// 保持 WSAConnect 的原生参数和错误形式，接管时记录原目标再转交连接入口。
unsafe extern "system" fn hook_wsa_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
    caller_data: *const c_void,
    callee_data: *mut c_void,
    sqos: *mut c_void,
    gqos: *mut c_void,
) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let originalWsaConnect: WsaConnectFn = original(&WSA_CONNECT, "WSA_CONNECT");
    if activity.isUnloading() {
        return originalWsaConnect(socket, name, name_len, caller_data, callee_data, sqos, gqos);
    }
    if let Some(ret) = proxy_connect(socket, name, name_len, |relay, relay_len| {
        originalWsaConnect(
            socket,
            relay,
            relay_len,
            std::ptr::null(),
            callee_data,
            sqos,
            gqos,
        )
    }) {
        if ret == 0 && !caller_data.is_null() {
            send_wsa_connect_caller_data(socket, caller_data);
        }
        return ret;
    }
    originalWsaConnect(socket, name, name_len, caller_data, callee_data, sqos, gqos)
}

// 连接同步成功后发送调用方提供的可选初始缓冲区；指针只在原 WSAConnect 调用期间借用。
unsafe fn send_wsa_connect_caller_data(socket: SOCKET, caller_data: *const c_void) {
    let buffer = caller_data as *const WSABUF;
    if buffer.is_null() || (*buffer).len == 0 || (*buffer).buf.is_null() {
        return;
    }
    let send: SendFn = original(&SEND, "SEND");
    let _ = send(
        socket,
        (*buffer).buf.0.cast(),
        (*buffer).len.min(i32::MAX as u32) as i32,
        0,
    );
}

// 通过已发布的 ConnectEx trampoline 调用原入口；只允许已进入对应 detour 的回调使用。
unsafe fn call_original_connect_ex(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
    send_buffer: *const c_void,
    send_data_len: u32,
    bytes_sent: *mut u32,
    overlapped: *mut c_void,
) -> BOOL {
    original::<ConnectExFn>(&CONNECT_EX, "CONNECT_EX")(
        socket,
        name,
        name_len,
        send_buffer,
        send_data_len,
        bytes_sent,
        overlapped,
    )
}

// 已缓存的 ConnectEx 指针也会进入此回调；保留异步完成状态，在第一次应用发送前补发 Relay 头。
unsafe extern "system" fn hook_connect_ex(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
    send_buffer: *const c_void,
    send_data_len: u32,
    bytes_sent: *mut u32,
    overlapped: *mut c_void,
) -> BOOL {
    let activity = super::hookInstall::CallbackActivity::enter();
    if activity.isUnloading() {
        return call_original_connect_ex(
            socket,
            name,
            name_len,
            send_buffer,
            send_data_len,
            bytes_sent,
            overlapped,
        );
    }
    let runtime = proxyRuntimeConfig();
    // 是否改连 relay、以及是否透传(目标走本机已知代理端口):
    // - 外部目标:改连 relay,发私有头;
    // - 本机已知代理端口:标记原代理，路由头后仍是目标自己的 HTTP CONNECT；
    // - 其它回环:直连放行。
    let target = sockaddr_target(name, name_len);
    let (should_redirect, passthrough) = match target {
        Some(t) if runtime.is_some() && socket_is_tcp(socket) => {
            if is_loopback_ip(t.ip) {
                if super::proxyDiscovery::isPlainHttpProxy(std::net::SocketAddr::new(t.ip, t.port))
                {
                    (true, true)
                } else {
                    (false, false)
                }
            } else {
                (true, false)
            }
        }
        _ => (false, false),
    };
    if should_redirect {
        let runtime = runtime.expect("已校验运行实例有效");
        // ConnectEx 携带初始数据 + overlapped 的异步形态难以在改连后保证“头/握手在数据前”,
        // 直接拒绝,迫使调用方回退到 connect + send(同样被 hook 接管)。
        if send_data_len > 0 && !overlapped.is_null() {
            WSASetLastError(WSAEOPNOTSUPP);
            return BOOL(0);
        }
        let target = target.expect("已校验目的地址有效");
        let Some(relay) = relay_sockaddr_for(name, name_len, runtime.relayPort) else {
            WSASetLastError(WSAEAFNOSUPPORT);
            return BOOL(0);
        };
        let result = call_original_connect_ex(
            socket,
            relay.as_ptr(),
            relay.len(),
            std::ptr::null(),
            0,
            bytes_sent,
            overlapped,
        );
        if result.as_bool() {
            // 同步完成先提交来源路由，再发送应用初始数据；返回的字节数只包含应用数据。
            remember_proxy_socket(socket, target, passthrough);
            if !ensure_proxy_header_sent(socket) {
                return BOOL(0);
            }
            if send_data_len > 0 && !send_buffer.is_null() {
                if !send_header_bytes(
                    socket,
                    std::slice::from_raw_parts(send_buffer as *const u8, send_data_len as usize),
                ) {
                    WSASetLastError(WSAECONNRESET);
                    return BOOL(0);
                }
                if !bytes_sent.is_null() {
                    *bytes_sent = send_data_len;
                }
            }
            return BOOL(1);
        }
        // 异步挂起只登记，来源路由头延后到首个 WSASend；不把代理连接误标为已经发送头部。
        let err = WSAGetLastError().0;
        if err == WSA_IO_PENDING {
            remember_proxy_socket(socket, target, passthrough);
        }
        WSASetLastError(err);
        return result;
    }
    call_original_connect_ex(
        socket,
        name,
        name_len,
        send_buffer,
        send_data_len,
        bytes_sent,
        overlapped,
    )
}

// closesocket 前摘除表项，防止后续数字句柄复用时继承旧目标与头部状态。
unsafe extern "system" fn hook_close_socket(socket: SOCKET) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let close: CloseSocketFn = original(&CLOSE_SOCKET, "CLOSE_SOCKET");
    if activity.isUnloading() {
        return close(socket);
    }
    forget_proxy_socket(socket);
    close(socket)
}

// shutdown 按调用方给定方向转交，结束连接的观测登记，不修改应用退出或子进程行为。
unsafe extern "system" fn hook_shutdown(socket: SOCKET, how: i32) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let shutdown: ShutdownFn = original(&SHUTDOWN, "SHUTDOWN");
    if activity.isUnloading() {
        return shutdown(socket, how);
    }
    forget_proxy_socket(socket);
    shutdown(socket, how)
}

// Winsock 是此 DLL 的静态导入依赖；只解析已加载模块，缺失时初始化失败，不额外增加加载引用。
unsafe fn proc_addr(module: PCWSTR, name: PCSTR) -> Option<*const ()> {
    let module = GetModuleHandleW(module).ok()?;
    GetProcAddress(module, name).map(|entry| entry as *const ())
}

/// 安装原生入口前先发布 trampoline，保证其他线程第一次进入回调时原调用槽已经存在；失败返回 false。
unsafe fn install(
    slot: &super::hookInstall::DetourSlot,
    name: PCSTR,
    detour: *const (),
    label: &str,
) -> bool {
    let Some(addr) = proc_addr(w!("Ws2_32.dll"), name) else {
        log(&format!("找不到 {label}"));
        return false;
    };
    match slot.install(addr, detour) {
        Ok(()) => {
            log(&format!("已 hook {label}"));
            true
        }
        Err(e) => {
            log(&format!("启用 {label} hook 失败: {e}"));
            false
        }
    }
}

/// 运行期解析真实 ConnectEx 指针并对其 inline-hook。
///
/// 关键:Chromium / libuv(Node)在启动早期通过 `WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER)`
/// 取一次 ConnectEx 指针并长期缓存。对**已经在运行**的目标做注入时缓存指针早已生成,
/// 单纯 hook 导出符或拦截后续 WSAIoctl 查询都拦不住它。但该缓存指针指向 mswsock 内部
/// 实现,与我们现查到的是**同一地址**,因此直接 inline-hook 这个地址即可覆盖**已缓存与
/// 后续所有** ConnectEx 调用,这是强制代理 Electron/Node 不被旁路的关键。
unsafe fn install_connect_ex_hook(stage: *mut u32) -> bool {
    // 直接用 GetProcAddress 解析 Ws2_32 导出的显式函数指针,绕开 windows 绑定对
    // socket/WSAIoctl 的 Result 包装与 feature 门控。
    type SocketFn = unsafe extern "system" fn(i32, i32, i32) -> SOCKET;
    type WsaStartupFn = unsafe extern "system" fn(u16, *mut c_void) -> i32;
    type WsaIoctlFn = unsafe extern "system" fn(
        SOCKET,
        u32,
        *const c_void,
        u32,
        *mut c_void,
        u32,
        *mut u32,
        *mut c_void,
        *mut c_void,
    ) -> i32;

    setInitializationStage(stage, 190);
    let ws2 = w!("Ws2_32.dll");
    let (Some(socket_fn), Some(startup_fn), Some(ioctl_fn)) = (
        proc_addr(ws2, s!("socket")),
        proc_addr(ws2, s!("WSAStartup")),
        proc_addr(ws2, s!("WSAIoctl")),
    ) else {
        log("ConnectEx 接管失败:无法解析 Ws2_32 导出");
        return false;
    };
    let socket_fn: SocketFn = std::mem::transmute(socket_fn);
    let startup_fn: WsaStartupFn = std::mem::transmute(startup_fn);
    let ioctl_fn: WsaIoctlFn = std::mem::transmute(ioctl_fn);

    // 引用计数式加载 Winsock(目标已加载则只 +1);wsadata 只需被写,不读。
    let mut wsadata = [0u8; 512];
    setInitializationStage(stage, 191);
    let _ = startup_fn(0x0202, wsadata.as_mut_ptr().cast());

    const AF_INET_I: i32 = 2;
    const SOCK_STREAM_I: i32 = 1;
    const IPPROTO_TCP_I: i32 = 6;
    setInitializationStage(stage, 192);
    let probe = socket_fn(AF_INET_I, SOCK_STREAM_I, IPPROTO_TCP_I);
    if probe.0 == usize::MAX {
        log("ConnectEx 接管失败:创建探测 socket 失败");
        return false;
    }
    let mut guid = WSAID_CONNECTEX;
    let mut func: *mut c_void = std::ptr::null_mut();
    let mut bytes = 0u32;
    setInitializationStage(stage, 193);
    let ret = ioctl_fn(
        probe,
        SIO_GET_EXTENSION_FUNCTION_POINTER,
        (&mut guid as *mut windows::core::GUID).cast(),
        std::mem::size_of::<windows::core::GUID>() as u32,
        (&mut func as *mut *mut c_void).cast(),
        std::mem::size_of::<*mut c_void>() as u32,
        &mut bytes,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    setInitializationStage(stage, 194);
    // closesocket 已完成入口改写；初始化探针直接走已发布 trampoline，避免把内部资源清理误算作业务回调。
    let _ = original::<CloseSocketFn>(&CLOSE_SOCKET, "CLOSE_SOCKET")(probe);
    if ret != 0 || func.is_null() {
        log("ConnectEx 接管失败:WSAIoctl 未返回函数指针");
        return false;
    }
    setInitializationStage(stage, 195);
    match CONNECT_EX.install(func.cast_const().cast(), hook_connect_ex as *const ()) {
        Ok(()) => {
            log("已 inline-hook ConnectEx(运行期解析指针,覆盖已缓存指针)");
            true
        }
        Err(e) => {
            log(&format!("启用 ConnectEx hook 失败: {e}"));
            false
        }
    }
}

// 初始化阶段写回部署上下文，宿主可区分入口解析、元数据和旧连接接入故障；空指针只用于单元测试。
unsafe fn setInitializationStage(stage: *mut u32, value: u32) {
    if !stage.is_null() {
        stage.write_volatile(value);
    }
}

// PE 初始化完成后由内存部署器同步调用；ConnectEx 是实际客户端路径，全部入口成功才发布模块就绪。
unsafe fn initialize(stage: *mut u32) -> bool {
    // 入口可以先于配置就绪；缺少配置时保持原调用，后续启动 Relay 后无需重复加载 DLL。
    setInitializationStage(stage, 10);
    let mut ready = match super::trustProvider::install() {
        Ok(()) => true,
        Err(error) => {
            log(&error);
            false
        }
    };
    setInitializationStage(stage, 11);
    ready &= install(
        &CONNECT,
        s!("connect"),
        hook_connect as *const (),
        "connect",
    );
    setInitializationStage(stage, 12);
    ready &= install(&SEND, s!("send"), hook_send as *const (), "send");
    setInitializationStage(stage, 13);
    ready &= install(&SEND_TO, s!("sendto"), hook_send_to as *const (), "sendto");
    setInitializationStage(stage, 14);
    ready &= install(
        &WSA_SEND,
        s!("WSASend"),
        hook_wsa_send as *const (),
        "WSASend",
    );
    setInitializationStage(stage, 15);
    ready &= install(
        &WSA_SEND_TO,
        s!("WSASendTo"),
        hook_wsa_send_to as *const (),
        "WSASendTo",
    );
    setInitializationStage(stage, 16);
    ready &= install(
        &WSA_CONNECT,
        s!("WSAConnect"),
        hook_wsa_connect as *const (),
        "WSAConnect",
    );
    setInitializationStage(stage, 17);
    ready &= install(
        &CLOSE_SOCKET,
        s!("closesocket"),
        hook_close_socket as *const (),
        "closesocket",
    );
    setInitializationStage(stage, 18);
    ready &= install(
        &SHUTDOWN,
        s!("shutdown"),
        hook_shutdown as *const (),
        "shutdown",
    );
    setInitializationStage(stage, 19);
    ready &= install_connect_ex_hook(stage);
    if ready {
        setInitializationStage(stage, 20);
        ready = super::runtimeMetadata::publish()
            .map_err(|error| log(error))
            .is_ok();
    }
    if ready {
        #[cfg(target_arch = "x86_64")]
        setInitializationStage(stage, 21);
        #[cfg(target_arch = "x86_64")]
        match super::nativeCompletion::install() {
            Ok(true) => log("原生完成事件入口已安装"),
            Ok(false) => log("当前构建没有匹配的原生完成事件布局，保留网络与文件来源"),
            Err(error) => {
                log(error);
                ready = false;
            }
        }
    }
    if ready {
        setInitializationStage(stage, 22);
        NETWORK_READY.store(true, Ordering::Release);
        super::warmConnections::reconnectOriginalProxy();
        setInitializationStage(stage, 23);
        signal_ready();
        setInitializationStage(stage, 24);
    } else {
        log("网络入口未全部安装，观测模块未就绪");
    }
    ready
}

// 网络入口按固定顺序恢复和释放，ConnectEx 与普通 Winsock 入口共享同一卸载事务。
fn networkSlots() -> [&'static super::hookInstall::DetourSlot; 9] {
    [
        &CONNECT,
        &SEND,
        &SEND_TO,
        &WSA_SEND,
        &WSA_SEND_TO,
        &WSA_CONNECT,
        &CONNECT_EX,
        &CLOSE_SOCKET,
        &SHUTDOWN,
    ]
}

// 命名事件句柄由模块自身拥有；关闭后宿主不会继续把待释放映像识别为已加载或就绪。
fn closePublishedEvent(event: &AtomicIsize) {
    let raw = event.swap(0, Ordering::AcqRel);
    if raw != 0 {
        unsafe {
            let _ = CloseHandle(HANDLE(raw as *mut _));
        }
    }
}

// 第一步只恢复所有目标入口；任一失败都保留映像，避免释放仍可能被调用的回调代码。
fn disableHooks() -> Result<(), String> {
    NETWORK_READY.store(false, Ordering::Release);
    super::trustProvider::disable()?;
    #[cfg(target_arch = "x86_64")]
    super::nativeCompletion::disable()?;
    for slot in networkSlots() {
        slot.disable()?;
    }
    Ok(())
}

// 活跃回调排空后释放 detour 页、证书缓存、运行目录映射和路由句柄。
fn releaseRuntime() -> Result<(), String> {
    #[cfg(target_arch = "x86_64")]
    super::nativeCompletion::release()?;
    super::trustProvider::release()?;
    for slot in networkSlots() {
        slot.release()?;
    }
    if let Ok(mut sockets) = proxy_sockets().lock() {
        sockets.clear();
    }
    if let Some(control) = RELAY_CONTROL.get() {
        control.lock().map_err(|_| "Relay 配置锁损坏")?.clear();
    }
    super::runtimeMetadata::shutdown();
    closePublishedEvent(&READY_EVENT);
    closePublishedEvent(&LOADED_EVENT);
    Ok(())
}

// CRT 入口只完成模块级运行库初始化；业务入口由部署器在相同 TLS 已登记线程上显式调用。
#[no_mangle]
pub extern "system" fn DllMain(_hinst: HMODULE, _reason: u32, _reserved: *mut c_void) -> BOOL {
    TRUE
}

// 内存部署器调用的唯一业务入口；成功后再发布加载事件，失败映像由宿主回收且不会阻塞下一轮部署。
#[no_mangle]
pub extern "system" fn observationInitialize(stage: *mut u32) -> u32 {
    if unsafe { initialize(stage) } {
        signal_loaded();
        1
    } else {
        0
    }
}

// 看门狗在目标进程内同步执行：恢复入口、排空回调、注销异常表后返回，宿主随后才释放主映像。
#[no_mangle]
pub extern "system" fn observationShutdown(context: *mut c_void) -> u32 {
    if context.is_null() {
        return 0;
    }
    let context = unsafe { *(context as *const cpcommon::deploymentLifecycle::UnloadContext) };
    if !super::hookInstall::beginUnload() {
        return 0;
    }
    if disableHooks().is_err() || !super::hookInstall::waitForCallbacks() {
        return 0;
    }
    if releaseRuntime().is_err() {
        return 0;
    }
    type EntryPoint = unsafe extern "system" fn(HMODULE, u32, *mut c_void) -> BOOL;
    let entryPoint: EntryPoint = unsafe { std::mem::transmute(context.entryPoint) };
    unsafe {
        let _ = entryPoint(
            HMODULE(context.imageBase as *mut _),
            0,
            std::ptr::null_mut(),
        );
    };
    if context.functionTable != 0
        && !unsafe {
            windows::Win32::System::Diagnostics::Debug::RtlDeleteFunctionTable(
                context.functionTable as *const _,
            )
            .as_bool()
        }
    {
        return 0;
    }
    1
}

#[cfg(test)]
#[path = "../tests/unit/networkStateTests.rs"]
mod tests;
