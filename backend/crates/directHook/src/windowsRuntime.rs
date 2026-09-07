//! Windows 网络回调与运行期配置；不修改原登录、时区、系统代理和子进程行为。
use super::relayControl::RelayControl;
use cpcommon::hook_proxy::{encode_header, HookProxyTarget};
use cpcommon::relayContract::RelayConfig;
use retour::GenericDetour;
use std::collections::HashMap;
use std::ffi::{c_void, OsString};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use windows::core::{s, w, PCSTR, PCWSTR, PSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HMODULE, TRUE};
use windows::Win32::Networking::WinSock::{
    getsockopt, WSAGetLastError, WSASetLastError, AF_INET, AF_INET6, SOCKADDR, SOCKET, SOCK_STREAM,
    SOL_SOCKET, SO_TYPE, WSABUF,
};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW, GetProcAddress};
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::System::Threading::{
    CreateEventW, CreateThread, SetEvent, THREAD_CREATION_FLAGS,
};
static HINST: AtomicIsize = AtomicIsize::new(0);
static NETWORK_READY: AtomicBool = AtomicBool::new(false);
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
fn proxyRuntimeConfig() -> Option<Arc<RelayConfig>> {
    if !NETWORK_READY.load(Ordering::Acquire) {
        return None;
    }
    static control: OnceLock<Mutex<RelayControl>> = OnceLock::new();
    let path = dll_dir()?.join("hook.json");
    control
        .get_or_init(|| Mutex::new(RelayControl::default()))
        .lock()
        .ok()?
        .read(&path)
}

// 根据 DllMain 记录的模块句柄定位自身路径，供配置和就绪事件共用；查询失败返回 None。
fn dll_path() -> Option<PathBuf> {
    let h = HMODULE(HINST.load(Ordering::SeqCst) as *mut c_void);
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(h, &mut buf) };
    if n == 0 {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(&buf[..n as usize])))
}

// 从实际 DLL 路径解析配置目录，路径缺失向调用方传递 None，不猜宿主进程的安装目录。
fn dll_dir() -> Option<PathBuf> {
    dll_path()?.parent().map(|d| d.to_path_buf())
}

// 在目标进程写入短生命周期诊断，调用方仅传入状态和 API 名称，不传认证数据或请求正文。
fn log(msg: &str) {
    use std::io::Write;
    if let Some(dir) = dll_dir() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("cphook.log"))
        {
            let _ = writeln!(f, "[pid {}] {msg}", std::process::id());
        }
    }
}

// 初始化完成后发布与本模块路径匹配的命名事件，句柄保留到进程退出，供宿主后续扫描打开。
fn signal_ready() {
    static READY_EVENT: AtomicIsize = AtomicIsize::new(0);
    let Some(module) = dll_path() else {
        log("读取模块就绪路径失败");
        return;
    };
    let name = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(
        std::process::id(),
        &module,
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
    header_sent: bool,
    failed: bool,
}

type SharedSocketState = Arc<Mutex<ProxySocketState>>;
static PROXY_SOCKETS: OnceLock<Mutex<HashMap<usize, SharedSocketState>>> = OnceLock::new();

// 全局表只用于定位连接状态，实际头部写入使用每连接锁，避免一个慢连接阻塞所有发送线程。
fn proxy_sockets() -> &'static Mutex<HashMap<usize, SharedSocketState>> {
    PROXY_SOCKETS.get_or_init(|| Mutex::new(HashMap::new()))
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
unsafe fn socket_is_tcp(socket: SOCKET) -> bool {
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
    let Some(send) = SEND.get() else {
        WSASetLastError(WSAECONNRESET);
        return false;
    };
    let mut sent = 0usize;
    // 私有头很小(32 字节)且 relay 在本机,发送缓冲几乎总能立即容纳。但非阻塞 socket
    // (Chromium/Node 常用)偶发 WSAEWOULDBLOCK 时不能当致命错误直接断连,否则会触发
    // “网络错误”。这里对 WouldBlock 做有界自旋等待,确保头在任何应用数据之前完整送出。
    let mut spins = 0u32;
    while sent < bytes.len() {
        let remaining = bytes.len() - sent;
        let chunk_len = remaining.min(i32::MAX as usize) as i32;
        let ret = send.call(socket, bytes[sent..].as_ptr().cast(), chunk_len, 0);
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
    let header = encode_header(&state.target);
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
fn remember_proxy_socket(socket: SOCKET, target: HookProxyTarget, header_sent: bool) {
    if let Ok(mut sockets) = proxy_sockets().lock() {
        sockets.insert(
            socket_key(socket),
            Arc::new(Mutex::new(ProxySocketState {
                target,
                header_sent,
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
    // 回环连接:只接管“连本机已知代理端口”(目标自己配置的 HTTP 代理,如 Clash 7890)的连接——
    // 把它改连到 relay 并以**透传模式**处理(目标会发 HTTP CONNECT,relay 解析真实目标后走内核),
    // 从底层把目标“自己走代理”的流量也彻底重定向过来,而不是拒绝。其它回环(本地 IPC、
    // devtools、本地服务)必须直连放行,否则会被错误塞进 relay。
    let passthrough = if is_loopback_ip(target.ip) {
        if runtime.loopbackProxyPorts.contains(&target.port) {
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
        if passthrough {
            // 透传:目标自己会发 HTTP CONNECT,不补私有头(header_sent=true 让 send hook 跳过)。
            remember_proxy_socket(socket, target, true);
        } else {
            remember_proxy_socket(socket, target, false);
            if !ensure_proxy_header_sent(socket) {
                return Some(-1);
            }
        }
        log(&format!(
            "TCP 已接入观测 Relay, target={}:{} relay=127.0.0.1:{} passthrough={}",
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
static CONNECT: OnceLock<GenericDetour<ConnectFn>> = OnceLock::new();
static SEND: OnceLock<GenericDetour<SendFn>> = OnceLock::new();
static SEND_TO: OnceLock<GenericDetour<SendToFn>> = OnceLock::new();
static WSA_SEND: OnceLock<GenericDetour<WsaSendFn>> = OnceLock::new();
static WSA_SEND_TO: OnceLock<GenericDetour<WsaSendToFn>> = OnceLock::new();
static WSA_CONNECT: OnceLock<GenericDetour<WsaConnectFn>> = OnceLock::new();
static CONNECT_EX: OnceLock<GenericDetour<ConnectExFn>> = OnceLock::new();
static CLOSE_SOCKET: OnceLock<GenericDetour<CloseSocketFn>> = OnceLock::new();
static SHUTDOWN: OnceLock<GenericDetour<ShutdownFn>> = OnceLock::new();
// connect 回调仅处理已启用的 TCP 目标；未接管连接及错误返回沿用 Winsock ABI。
unsafe extern "system" fn hook_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
) -> i32 {
    let detour = CONNECT.get().expect("CONNECT 未初始化");
    if let Some(ret) = proxy_connect(socket, name, name_len, |relay, relay_len| {
        detour.call(socket, relay, relay_len)
    }) {
        return ret;
    }
    detour.call(socket, name, name_len)
}

// send 回调先提交一次性 Relay 头，再调用原发送函数；头失败时返回 SOCKET_ERROR。
unsafe extern "system" fn hook_send(
    socket: SOCKET,
    buffer: *const c_void,
    len: i32,
    flags: i32,
) -> i32 {
    // 只为已注册的 TCP 连接补发私有头，UDP 和其他未接管连接保留原始发送行为。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    SEND.get()
        .expect("SEND 未初始化")
        .call(socket, buffer, len, flags)
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
    // Winsock 也允许 TCP 通过 sendto 发送；与 send 共用一次性私有头，未注册的 UDP 保持原样。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    SEND_TO
        .get()
        .expect("SEND_TO 未初始化")
        .call(socket, buffer, len, flags, to, to_len)
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
    // 同 hook_send:UDP 不在此拦截,只负责首个 TCP 应用数据前补发私有头。
    if !ensure_proxy_header_sent(socket) {
        if !bytes_sent.is_null() {
            *bytes_sent = 0;
        }
        return -1;
    }
    WSA_SEND.get().expect("WSA_SEND 未初始化").call(
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
    // 注册 TCP 连接同样先发送私有头，不修改 UDP、DNS 或应用自己的代理选择。
    if !ensure_proxy_header_sent(socket) {
        return -1;
    }
    WSA_SEND_TO.get().expect("WSA_SEND_TO 未初始化").call(
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
    let detour = WSA_CONNECT.get().expect("WSA_CONNECT 未初始化");
    if let Some(ret) = proxy_connect(socket, name, name_len, |relay, relay_len| {
        detour.call(
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
    detour.call(socket, name, name_len, caller_data, callee_data, sqos, gqos)
}

// 连接同步成功后发送调用方提供的可选初始缓冲区；指针只在原 WSAConnect 调用期间借用。
unsafe fn send_wsa_connect_caller_data(socket: SOCKET, caller_data: *const c_void) {
    let buffer = caller_data as *const WSABUF;
    if buffer.is_null() || (*buffer).len == 0 || (*buffer).buf.is_null() {
        return;
    }
    let _ = SEND.get().map(|send| {
        send.call(
            socket,
            (*buffer).buf.0.cast(),
            (*buffer).len.min(i32::MAX as u32) as i32,
            0,
        )
    });
}

// 通过已发布的 ConnectEx trampoline 调用原入口；初始化缺失返回明确 Winsock 错误。
unsafe fn call_original_connect_ex(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_len: i32,
    send_buffer: *const c_void,
    send_data_len: u32,
    bytes_sent: *mut u32,
    overlapped: *mut c_void,
) -> BOOL {
    let Some(detour) = CONNECT_EX.get() else {
        WSASetLastError(WSAEOPNOTSUPP);
        return BOOL(0);
    };
    detour.call(
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
    let runtime = proxyRuntimeConfig();
    // 是否改连 relay、以及是否透传(目标走本机已知代理端口):
    // - 外部目标:改连 relay,发私有头;
    // - 本机已知代理端口:改连 relay,透传(目标自己会发 HTTP CONNECT);
    // - 其它回环:直连放行。
    let target = sockaddr_target(name, name_len);
    let (should_redirect, passthrough) = match target {
        Some(t) if runtime.is_some() && socket_is_tcp(socket) => {
            if is_loopback_ip(t.ip) {
                if runtime
                    .as_ref()
                    .is_some_and(|settings| settings.loopbackProxyPorts.contains(&t.port))
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
            // 同步完成:登记;非透传立即补发私有头;两种模式都把可选初始数据原样发出。
            remember_proxy_socket(socket, target, passthrough);
            if !passthrough && !ensure_proxy_header_sent(socket) {
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
        // 异步挂起:仅登记;非透传的私有头延后到首个 WSASend 由 ensure_proxy_header_sent 补发,
        // 透传(header_sent=true)则让目标自己的 HTTP CONNECT 原样到达 relay。
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
    forget_proxy_socket(socket);
    CLOSE_SOCKET
        .get()
        .expect("CLOSE_SOCKET 未初始化")
        .call(socket)
}

// shutdown 按调用方给定方向转交，结束连接的观测登记，不修改应用退出或子进程行为。
unsafe extern "system" fn hook_shutdown(socket: SOCKET, how: i32) -> i32 {
    forget_proxy_socket(socket);
    SHUTDOWN.get().expect("SHUTDOWN 未初始化").call(socket, how)
}

// Winsock 是此 DLL 的静态导入依赖；只解析已加载模块，缺失时初始化失败，不额外增加加载引用。
unsafe fn proc_addr(module: PCWSTR, name: PCSTR) -> Option<*const ()> {
    let module = GetModuleHandleW(module).ok()?;
    GetProcAddress(module, name).map(|entry| entry as *const ())
}

/// 安装原生入口前先发布 trampoline，保证其他线程第一次进入回调时原调用槽已经存在；失败返回 false。
unsafe fn install<F: retour::Function + Copy>(
    slot: &OnceLock<GenericDetour<F>>,
    name: PCSTR,
    detour: F,
    label: &str,
) -> bool {
    let Some(addr) = proc_addr(w!("Ws2_32.dll"), name) else {
        log(&format!("找不到 {label}"));
        return false;
    };
    let target: F = std::mem::transmute_copy(&addr);
    match GenericDetour::new(target, detour) {
        Ok(det) => match super::hookInstall::activateDetour(slot, det) {
            Ok(()) => {
                log(&format!("已 hook {label}"));
                true
            }
            Err(e) => {
                log(&format!("启用 {label} hook 失败: {e}"));
                false
            }
        },
        Err(e) => {
            log(&format!("创建 {label} detour 失败: {e}"));
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
unsafe fn install_connect_ex_hook() -> bool {
    // 直接用 GetProcAddress 解析 Ws2_32 导出的显式函数指针,绕开 windows 绑定对
    // socket/WSAIoctl 的 Result 包装与 feature 门控。
    type SocketFn = unsafe extern "system" fn(i32, i32, i32) -> SOCKET;
    type ClosesocketFn = unsafe extern "system" fn(SOCKET) -> i32;
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

    let ws2 = w!("Ws2_32.dll");
    let (Some(socket_fn), Some(close_fn), Some(startup_fn), Some(ioctl_fn)) = (
        proc_addr(ws2, s!("socket")),
        proc_addr(ws2, s!("closesocket")),
        proc_addr(ws2, s!("WSAStartup")),
        proc_addr(ws2, s!("WSAIoctl")),
    ) else {
        log("ConnectEx 接管失败:无法解析 Ws2_32 导出");
        return false;
    };
    let socket_fn: SocketFn = std::mem::transmute(socket_fn);
    let close_fn: ClosesocketFn = std::mem::transmute(close_fn);
    let startup_fn: WsaStartupFn = std::mem::transmute(startup_fn);
    let ioctl_fn: WsaIoctlFn = std::mem::transmute(ioctl_fn);

    // 引用计数式加载 Winsock(目标已加载则只 +1);wsadata 只需被写,不读。
    let mut wsadata = [0u8; 512];
    let _ = startup_fn(0x0202, wsadata.as_mut_ptr().cast());

    const AF_INET_I: i32 = 2;
    const SOCK_STREAM_I: i32 = 1;
    const IPPROTO_TCP_I: i32 = 6;
    let probe = socket_fn(AF_INET_I, SOCK_STREAM_I, IPPROTO_TCP_I);
    if probe.0 == usize::MAX {
        log("ConnectEx 接管失败:创建探测 socket 失败");
        return false;
    }
    let mut guid = WSAID_CONNECTEX;
    let mut func: *mut c_void = std::ptr::null_mut();
    let mut bytes = 0u32;
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
    let _ = close_fn(probe);
    if ret != 0 || func.is_null() {
        log("ConnectEx 接管失败:WSAIoctl 未返回函数指针");
        return false;
    }
    let target: ConnectExFn = std::mem::transmute(func);
    match GenericDetour::new(target, hook_connect_ex as ConnectExFn) {
        Ok(det) => match super::hookInstall::activateDetour(&CONNECT_EX, det) {
            Ok(()) => {
                log("已 inline-hook ConnectEx(运行期解析指针,覆盖已缓存指针)");
                true
            }
            Err(e) => {
                log(&format!("启用 ConnectEx hook 失败: {e}"));
                false
            }
        },
        Err(e) => {
            log(&format!("创建 ConnectEx detour 失败: {e}"));
            false
        }
    }
}

// loader lock 外安装网络入口；ConnectEx 是实际客户端路径，必须成功才能发布模块就绪。
unsafe extern "system" fn worker(_: *mut c_void) -> u32 {
    // 入口可以先于配置就绪；缺少配置时保持原调用，后续启动 Relay 后无需重复加载 DLL。
    let mut ready = true;
    ready &= install(
        &CONNECT,
        s!("connect"),
        hook_connect as ConnectFn,
        "connect",
    );
    ready &= install(&SEND, s!("send"), hook_send as SendFn, "send");
    ready &= install(&SEND_TO, s!("sendto"), hook_send_to as SendToFn, "sendto");
    ready &= install(
        &WSA_SEND,
        s!("WSASend"),
        hook_wsa_send as WsaSendFn,
        "WSASend",
    );
    ready &= install(
        &WSA_SEND_TO,
        s!("WSASendTo"),
        hook_wsa_send_to as WsaSendToFn,
        "WSASendTo",
    );
    ready &= install(
        &WSA_CONNECT,
        s!("WSAConnect"),
        hook_wsa_connect as WsaConnectFn,
        "WSAConnect",
    );
    ready &= install(
        &CLOSE_SOCKET,
        s!("closesocket"),
        hook_close_socket as CloseSocketFn,
        "closesocket",
    );
    ready &= install(
        &SHUTDOWN,
        s!("shutdown"),
        hook_shutdown as ShutdownFn,
        "shutdown",
    );
    ready &= install_connect_ex_hook();
    if ready {
        NETWORK_READY.store(true, Ordering::Release);
        signal_ready();
    } else {
        log("网络入口未全部安装，观测模块未就绪");
    }
    0
}

// DLL 回调只启动初始化线程；创建线程失败让加载失败，不允许宿主把未执行初始化当作成功。
#[no_mangle]
pub extern "system" fn DllMain(hinst: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        HINST.store(hinst.0 as isize, Ordering::SeqCst);
        unsafe {
            match CreateThread(None, 0, Some(worker), None, THREAD_CREATION_FLAGS(0), None) {
                Ok(thread) => {
                    let _ = CloseHandle(thread);
                }
                Err(_) => return BOOL(0),
            }
        }
    }
    TRUE
}

#[cfg(test)]
#[path = "../tests/unit/networkStateTests.rs"]
mod tests;
