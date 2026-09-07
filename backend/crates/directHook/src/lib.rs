//! cphook.dll —— 注入到目标进程后,在**进程内**用 inline hook 改写系统 API 的返回,
//! 让目标进程上报的**时区/语言**与代理出口地理一致,并阻断目标绕行本机代理。
//!
//! 设计:DllMain 仅起一个工作线程(避开 loader lock),线程里读取与本 DLL 同目录的
//! `hook.json`,再用 retour 对 GetTimeZoneInformation / GetDynamicTimeZoneInformation /
//! GetUserDefaultLocaleName / GetSystemDefaultLocaleName 安装 inline hook。

#[cfg(windows)]
mod imp {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::ffi::{c_void, CStr, OsString};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, SystemTime};

    use cpcommon::hook_proxy::{encode_header, HookProxyTarget};
    use retour::GenericDetour;
    use serde::Deserialize;
    use windows::core::{s, w, PCSTR, PCWSTR, PSTR, PWSTR};
    use windows::Win32::Foundation::{
        CloseHandle, SetLastError, BOOL, ERROR_ACCESS_DENIED, ERROR_ENVVAR_NOT_FOUND,
        ERROR_SUCCESS, HANDLE, HMODULE, TRUE, WAIT_OBJECT_0,
    };
    use windows::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
    };
    use windows::Win32::Networking::WinInet::{
        INTERNET_OPTION_PER_CONNECTION_OPTION, INTERNET_PER_CONN_FLAGS,
        INTERNET_PER_CONN_OPTION_LISTW, PROXY_TYPE_DIRECT,
    };
    use windows::Win32::Networking::WinSock::{
        getsockopt, WSAGetLastError, WSASetLastError, AF_INET, AF_INET6, SOCKADDR, SOCKET,
        SOCK_DGRAM, SOCK_STREAM, SOL_SOCKET, SO_TYPE, WSABUF,
    };
    use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleW, GetProcAddress, LoadLibraryW,
    };
    use windows::Win32::System::Memory::{
        VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
    };
    use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
    use windows::Win32::System::Threading::{
        CreateEventW, CreateRemoteThread, CreateThread, GetExitCodeThread, OpenEventW,
        ResumeThread, SetEvent, TerminateProcess, WaitForSingleObject, CREATE_SUSPENDED,
        LPTHREAD_START_ROUTINE, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOA,
        STARTUPINFOW, SYNCHRONIZATION_SYNCHRONIZE, THREAD_CREATION_FLAGS,
    };
    use windows::Win32::System::Time::{DYNAMIC_TIME_ZONE_INFORMATION, TIME_ZONE_INFORMATION};

    static HINST: AtomicIsize = AtomicIsize::new(0);
    const SIO_GET_EXTENSION_FUNCTION_POINTER: u32 = 0xC8000006;
    const WSAID_CONNECTEX: windows::core::GUID =
        windows::core::GUID::from_u128(0x25a207b9_ddf3_4660_8ee9_76e58c74063e);
    const WSAEACCES: i32 = 10013;
    const WSAEINPROGRESS: i32 = 10036;
    const WSAEOPNOTSUPP: i32 = 10045;
    const WSAEWOULDBLOCK: i32 = 10035;
    const WSAECONNRESET: i32 = 10054;
    const WSAEAFNOSUPPORT: i32 = 10047;
    const WSA_IO_PENDING: i32 = 997;
    const CHILD_INJECTION_TIMEOUT_MS: u32 = 10_000;
    const CHILD_READY_TIMEOUT_MS: u32 = 8_000;
    const CHILD_READY_OPEN_RETRIES: usize = 200;

    /// 与出口地理一致的伪装画像(由 host 写到与本 DLL 同目录的 hook.json)。
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct HookConfig {
        enabled: bool,
        /// 相对 UTC 的分钟数(UTC-8 => 480)。
        tz_bias: i32,
        /// 标准时区显示名(如 "Pacific Standard Time")。
        tz_std_name: String,
        /// 夏令时显示名(如 "Pacific Daylight Time")。
        tz_dst_name: String,
        /// 注册表时区键名(ICU 用它映射到 IANA,如 "Pacific Standard Time")。
        tz_key_name: String,
        /// 区域名(如 "en-US")。
        locale: String,
        /// 隐藏目标进程里的代理环境变量,避免 Node/undici 自动走系统外部代理。
        clear_proxy_env: bool,
        /// 禁止目标进程连接这些本机代理端口,防止代理环境或系统代理绕回 Clash。
        blocked_loopback_proxy_ports: Vec<u16>,
        /// 本地 relay 端口。0 表示不启用强制代理,只保留指纹与代理环境清理 hook。
        proxy_relay_port: u16,
        /// 是否把目标 TCP 改连到 Cproxy relay。
        force_proxy_tcp: bool,
        /// 是否阻断目标 UDP。强制代理运行时必须开启,否则 QUIC/UDP 会绕过 relay。
        block_udp: bool,
    }

    fn config() -> &'static Option<HookConfig> {
        static CFG: OnceLock<Option<HookConfig>> = OnceLock::new();
        CFG.get_or_init(|| {
            let path = dll_dir()?.join("hook.json");
            let bytes = std::fs::read(path).ok()?;
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
            serde_json::from_slice::<HookConfig>(bytes).ok()
        })
    }

    #[derive(Clone, Copy)]
    struct ProxyRuntimeConfig {
        relay_port: u16,
        force_proxy_tcp: bool,
        block_udp: bool,
    }

    struct RuntimeConfigCache {
        modified: Option<SystemTime>,
        config: ProxyRuntimeConfig,
    }

    fn proxy_runtime_config() -> ProxyRuntimeConfig {
        static CACHE: OnceLock<Mutex<RuntimeConfigCache>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| {
            Mutex::new(RuntimeConfigCache {
                modified: None,
                config: ProxyRuntimeConfig {
                    relay_port: config()
                        .as_ref()
                        .map(|cfg| cfg.proxy_relay_port)
                        .unwrap_or(0),
                    force_proxy_tcp: config()
                        .as_ref()
                        .map(|cfg| cfg.force_proxy_tcp)
                        .unwrap_or(false),
                    block_udp: config().as_ref().map(|cfg| cfg.block_udp).unwrap_or(false),
                },
            })
        });
        let Ok(mut guard) = cache.lock() else {
            return ProxyRuntimeConfig {
                relay_port: 0,
                force_proxy_tcp: false,
                block_udp: false,
            };
        };
        let Some(path) = dll_dir().map(|dir| dir.join("hook.json")) else {
            return guard.config;
        };
        let modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok();
        if modified != guard.modified {
            if let Ok(bytes) = std::fs::read(&path) {
                let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
                if let Ok(cfg) = serde_json::from_slice::<HookConfig>(bytes) {
                    guard.config = ProxyRuntimeConfig {
                        relay_port: cfg.proxy_relay_port,
                        force_proxy_tcp: cfg.force_proxy_tcp && cfg.proxy_relay_port != 0,
                        block_udp: cfg.block_udp,
                    };
                    guard.modified = modified;
                }
            }
        }
        guard.config
    }

    fn dll_path() -> Option<PathBuf> {
        let h = HMODULE(HINST.load(Ordering::SeqCst) as *mut c_void);
        let mut buf = [0u16; 1024];
        let n = unsafe { GetModuleFileNameW(h, &mut buf) };
        if n == 0 {
            return None;
        }
        Some(PathBuf::from(OsString::from_wide(&buf[..n as usize])))
    }

    fn dll_dir() -> Option<PathBuf> {
        dll_path()?.parent().map(|d| d.to_path_buf())
    }

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

    fn signal_ready() {
        static READY_EVENT: AtomicIsize = AtomicIsize::new(0);
        let name =
            windows::core::HSTRING::from(cpcommon::hook_ready::event_name(std::process::id()));
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

    fn proxy_env_name(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "http_proxy"
                | "https_proxy"
                | "all_proxy"
                | "no_proxy"
                | "node_use_env_proxy"
                | "node_use_system_proxy"
        )
    }

    fn should_hide_env(name: &str) -> bool {
        config()
            .as_ref()
            .map(|cfg| cfg.clear_proxy_env && proxy_env_name(name))
            .unwrap_or(false)
    }

    unsafe fn read_pcwstr(name: PCWSTR) -> Option<String> {
        if name.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *name.0.add(len) != 0 && len < 32_768 {
            len += 1;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            name.0, len,
        )))
    }

    unsafe fn read_pcstr(name: PCSTR) -> Option<String> {
        if name.is_null() {
            return None;
        }
        Some(
            CStr::from_ptr(name.0 as *const i8)
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn blocked_proxy_port(port: u16) -> bool {
        config()
            .as_ref()
            .map(|cfg| cfg.blocked_loopback_proxy_ports.contains(&port))
            .unwrap_or(false)
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
    }

    static PROXY_SOCKETS: OnceLock<Mutex<HashMap<usize, ProxySocketState>>> = OnceLock::new();

    fn proxy_sockets() -> &'static Mutex<HashMap<usize, ProxySocketState>> {
        PROXY_SOCKETS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn socket_key(socket: SOCKET) -> usize {
        socket.0
    }

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

    fn is_loopback_ip(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ip) => ip.is_loopback(),
            IpAddr::V6(ip) => ip.is_loopback(),
        }
    }

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

    unsafe fn socket_is_udp(socket: SOCKET) -> bool {
        let mut socket_type = 0_i32;
        let mut socket_type_len = std::mem::size_of::<i32>() as i32;
        let ret = getsockopt(
            socket,
            SOL_SOCKET,
            SO_TYPE,
            PSTR((&mut socket_type as *mut i32).cast()),
            &mut socket_type_len,
        );
        ret == 0 && socket_type == SOCK_DGRAM.0
    }

    const DNS_PORT: u16 = 53;

    /// 强制代理运行时阻断目标 UDP,但放行 DNS(53):既不封死进程内 c-ares DNS,
    /// 又仍阻断 QUIC(UDP/443)等绕过 TCP 代理的流量。判不出目的端口时按阻断处理。
    /// 只在 connect / sendto 这类“能看到目的地址”的入口判定;连接态 UDP 的 send 由 connect 时已门控。
    unsafe fn should_block_udp(
        socket: SOCKET,
        dest: *const SOCKADDR,
        dest_len: i32,
        label: &str,
    ) -> bool {
        if !proxy_runtime_config().block_udp {
            return false;
        }
        if !socket_is_udp(socket) {
            return false;
        }
        if let Some(target) = sockaddr_target(dest, dest_len) {
            if target.port == DNS_PORT {
                return false;
            }
        }
        log(&format!("已阻断目标 UDP {label}"));
        WSASetLastError(WSAEACCES);
        true
    }

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

    unsafe fn send_proxy_header(socket: SOCKET, state: ProxySocketState) -> bool {
        let header = encode_header(&state.target);
        if send_header_bytes(socket, &header) {
            true
        } else {
            WSASetLastError(WSAECONNRESET);
            false
        }
    }

    unsafe fn ensure_proxy_header_sent(socket: SOCKET) -> bool {
        let key = socket_key(socket);
        let state = {
            let Ok(mut sockets) = proxy_sockets().lock() else {
                WSASetLastError(WSAECONNRESET);
                return false;
            };
            let Some(state) = sockets.get_mut(&key) else {
                return true;
            };
            if state.header_sent {
                return true;
            }
            *state
        };
        if !send_proxy_header(socket, state) {
            // 私有头发送失败后这条 relay 连接已经不能保持协议同步;立即摘掉状态,
            // 防止后续应用重试 send 时反复持有同一条失败记录造成目标进程内状态表增长。
            forget_proxy_socket(socket);
            return false;
        }
        if let Ok(mut sockets) = proxy_sockets().lock() {
            if let Some(current) = sockets.get_mut(&key) {
                current.header_sent = true;
            }
        }
        true
    }

    fn remember_proxy_socket(socket: SOCKET, target: HookProxyTarget, header_sent: bool) {
        if let Ok(mut sockets) = proxy_sockets().lock() {
            sockets.insert(
                socket_key(socket),
                ProxySocketState {
                    target,
                    header_sent,
                },
            );
        }
    }

    fn forget_proxy_socket(socket: SOCKET) {
        if let Ok(mut sockets) = proxy_sockets().lock() {
            sockets.remove(&socket_key(socket));
        }
    }

    unsafe fn proxy_connect(
        socket: SOCKET,
        name: *const SOCKADDR,
        name_len: i32,
        original_connect: impl FnOnce(*const SOCKADDR, i32) -> i32,
    ) -> Option<i32> {
        let runtime = proxy_runtime_config();
        if !runtime.force_proxy_tcp {
            return None;
        }
        if !socket_is_tcp(socket) {
            return None;
        }
        let target = sockaddr_target(name, name_len)?;
        // 回环连接:只接管“连本机已知代理端口”(目标自己配置的 HTTP 代理,如 Clash 7890)的连接——
        // 把它改连到 relay 并以**透传模式**处理(目标会发 HTTP CONNECT,relay 解析真实目标后走内核),
        // 从底层把目标“自己走代理”的流量也彻底重定向过来,而不是拒绝。其它回环(本地 IPC、
        // devtools、本地服务)必须直连放行,否则会被错误塞进 relay。
        let passthrough = if is_loopback_ip(target.ip) {
            if blocked_proxy_port(target.port) {
                true
            } else {
                return None;
            }
        } else {
            false
        };
        let Some(relay) = relay_sockaddr_for(name, name_len, runtime.relay_port) else {
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
                "TCP 已改连 Cproxy relay, target={}:{} relay=127.0.0.1:{} passthrough={}",
                target.ip, target.port, runtime.relay_port, passthrough
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

    /// 把 UTF-8 字符串写入定长 UTF-16 数组(NUL 结尾,超长截断)。
    fn write_wide(dst: &mut [u16], s: &str) {
        let src: Vec<u16> = s.encode_utf16().collect();
        let n = src.len().min(dst.len().saturating_sub(1));
        dst[..n].copy_from_slice(&src[..n]);
        for c in &mut dst[n..] {
            *c = 0;
        }
    }

    // ===== hook 目标函数类型 =====
    type GetTziFn = unsafe extern "system" fn(*mut TIME_ZONE_INFORMATION) -> u32;
    type GetDynTziFn = unsafe extern "system" fn(*mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32;
    type GetLocaleFn = unsafe extern "system" fn(PWSTR, i32) -> i32;
    type GetEnvWFn = unsafe extern "system" fn(PCWSTR, PWSTR, u32) -> u32;
    type GetEnvAFn = unsafe extern "system" fn(PCSTR, PSTR, u32) -> u32;
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
    type WinHttpGetIeProxyConfigFn =
        unsafe extern "system" fn(*mut WINHTTP_CURRENT_USER_IE_PROXY_CONFIG) -> BOOL;
    type WinHttpGetProxyForUrlFn = unsafe extern "system" fn(
        *mut c_void,
        PCWSTR,
        *const c_void,
        *mut WINHTTP_PROXY_INFO,
    ) -> BOOL;
    type WinHttpOpenFn = unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, u32) -> *mut c_void;
    type InternetQueryOptionWFn =
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, *mut u32) -> BOOL;
    type CreateProcessWFn = unsafe extern "system" fn(
        PCWSTR,
        PWSTR,
        *const c_void,
        *const c_void,
        BOOL,
        PROCESS_CREATION_FLAGS,
        *const c_void,
        PCWSTR,
        *const STARTUPINFOW,
        *mut PROCESS_INFORMATION,
    ) -> BOOL;
    type CreateProcessAFn = unsafe extern "system" fn(
        PCSTR,
        PSTR,
        *const c_void,
        *const c_void,
        BOOL,
        PROCESS_CREATION_FLAGS,
        *const c_void,
        PCSTR,
        *const STARTUPINFOA,
        *mut PROCESS_INFORMATION,
    ) -> BOOL;

    static GET_TZI: OnceLock<GenericDetour<GetTziFn>> = OnceLock::new();
    static GET_DYN_TZI: OnceLock<GenericDetour<GetDynTziFn>> = OnceLock::new();
    static GET_USER_LOCALE: OnceLock<GenericDetour<GetLocaleFn>> = OnceLock::new();
    static GET_SYS_LOCALE: OnceLock<GenericDetour<GetLocaleFn>> = OnceLock::new();
    static GET_ENV_W: OnceLock<GenericDetour<GetEnvWFn>> = OnceLock::new();
    static GET_ENV_A: OnceLock<GenericDetour<GetEnvAFn>> = OnceLock::new();
    static CONNECT: OnceLock<GenericDetour<ConnectFn>> = OnceLock::new();
    static SEND: OnceLock<GenericDetour<SendFn>> = OnceLock::new();
    static SEND_TO: OnceLock<GenericDetour<SendToFn>> = OnceLock::new();
    static WSA_SEND: OnceLock<GenericDetour<WsaSendFn>> = OnceLock::new();
    static WSA_SEND_TO: OnceLock<GenericDetour<WsaSendToFn>> = OnceLock::new();
    static WSA_CONNECT: OnceLock<GenericDetour<WsaConnectFn>> = OnceLock::new();
    static CONNECT_EX: OnceLock<GenericDetour<ConnectExFn>> = OnceLock::new();
    static CLOSE_SOCKET: OnceLock<GenericDetour<CloseSocketFn>> = OnceLock::new();
    static SHUTDOWN: OnceLock<GenericDetour<ShutdownFn>> = OnceLock::new();
    static WINHTTP_IE_PROXY: OnceLock<GenericDetour<WinHttpGetIeProxyConfigFn>> = OnceLock::new();
    static WINHTTP_PROXY_FOR_URL: OnceLock<GenericDetour<WinHttpGetProxyForUrlFn>> =
        OnceLock::new();
    static WINHTTP_OPEN: OnceLock<GenericDetour<WinHttpOpenFn>> = OnceLock::new();
    static INTERNET_QUERY_OPTION_W: OnceLock<GenericDetour<InternetQueryOptionWFn>> =
        OnceLock::new();
    static CREATE_PROCESS_W: OnceLock<GenericDetour<CreateProcessWFn>> = OnceLock::new();
    static CREATE_PROCESS_A: OnceLock<GenericDetour<CreateProcessAFn>> = OnceLock::new();

    fn process_flags_with_suspended(flags: PROCESS_CREATION_FLAGS) -> PROCESS_CREATION_FLAGS {
        PROCESS_CREATION_FLAGS(flags.0 | CREATE_SUSPENDED.0)
    }

    fn process_flags_are_suspended(flags: PROCESS_CREATION_FLAGS) -> bool {
        flags.0 & CREATE_SUSPENDED.0 != 0
    }

    fn path_leaf_lower(path: &str) -> String {
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
    }

    fn first_command_token(command_line: &str) -> Option<&str> {
        let trimmed = command_line.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix('"') {
            return rest.find('"').map(|end| &rest[..end]);
        }
        trimmed.split_whitespace().next()
    }

    fn command_line_launches_self(command_line: Option<&str>, self_exe: &str) -> bool {
        let Some(token) = command_line.and_then(first_command_token) else {
            return false;
        };
        if token.contains(['\\', '/']) {
            token.eq_ignore_ascii_case(self_exe)
        } else {
            path_leaf_lower(token) == path_leaf_lower(self_exe)
        }
    }

    /// 当前进程主模块(exe)完整路径,缓存。用于识别“目标自引用启动内部工具”的调用。
    fn self_exe_path() -> Option<String> {
        static EXE: OnceLock<Option<String>> = OnceLock::new();
        EXE.get_or_init(|| {
            let mut buf = [0u16; 1024];
            let n = unsafe { GetModuleFileNameW(HMODULE::default(), &mut buf) };
            if n == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..n as usize]))
        })
        .clone()
    }

    /// CreateProcess(application=自身 exe, commandLine=非自身首参)表示“把自身镜像当内部工具壳”
    /// (如 claude.exe 以 argv0=rg 进入内置 ripgrep)。这类场景才放行;真正的自我重启/自拉起
    /// commandLine 首参仍是自身 exe,必须继续注入,否则子进程会绕过强制代理。
    unsafe fn is_self_internal_tool_w(application_name: PCWSTR, command_line: PWSTR) -> bool {
        let Some(app) = read_pcwstr(application_name) else {
            return false;
        };
        let Some(exe) = self_exe_path() else {
            return false;
        };
        app.eq_ignore_ascii_case(&exe)
            && !command_line_launches_self(read_pcwstr(PCWSTR(command_line.0)).as_deref(), &exe)
    }

    unsafe fn is_self_internal_tool_a(application_name: PCSTR, command_line: PSTR) -> bool {
        let Some(app) = read_pcstr(application_name) else {
            return false;
        };
        let Some(exe) = self_exe_path() else {
            return false;
        };
        app.eq_ignore_ascii_case(&exe)
            && !command_line_launches_self(read_pcstr(PCSTR(command_line.0)).as_deref(), &exe)
    }

    // ===== 自引用启动的 IFEO 自检屏蔽 =====
    //
    // 目标“用自己当内部工具”(如 claude 用 claude.exe 当内置 ripgrep:
    // application=claude.exe、argv0="rg")时,会在**本进程内**调 CreateProcess(claude.exe, "rg …")。
    // CreateProcess 创建进程时,系统(kernelbase → ntdll)用 NtOpenKey 打开
    // `…\Image File Execution Options\<自身镜像名>` 读 `Debugger`,于是这次自引用启动也被 IFEO
    // 重定向到 `Cproxy --shim`;而重定向会丢掉 application_name,只把命令行 `rg …` 交给 shim,
    // shim 拿 `rg` 当可执行去启动必然失败(rg 只是 claude.exe 的内置模式)。
    //
    // 解决:cphook 就在目标进程内,可拦下这次“IFEO 自检”——仅当我们识别为自引用启动
    // (CreateProcess 的 application 就是自身 exe)、且在同一调用线程内时,对“打开自身镜像 IFEO 子键”
    // 返回“键不存在”,使系统按“无 IFEO 配置”直接启动 claude.exe(rg 模式),不重定向。
    // 屏蔽只在被注入的本进程本线程内生效:外部 / 系统启动任何 claude 读的是真实注册表(键在)
    // → 仍正常被 IFEO 接管,无副作用。

    const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }

    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: HANDLE,
        object_name: *const UnicodeString,
        attributes: u32,
        security_descriptor: *mut c_void,
        security_quality_of_service: *mut c_void,
    }

    type NtOpenKeyFn = unsafe extern "system" fn(*mut HANDLE, u32, *const ObjectAttributes) -> i32;
    type NtOpenKeyExFn =
        unsafe extern "system" fn(*mut HANDLE, u32, *const ObjectAttributes, u32) -> i32;

    static NT_OPEN_KEY: OnceLock<GenericDetour<NtOpenKeyFn>> = OnceLock::new();
    static NT_OPEN_KEY_EX: OnceLock<GenericDetour<NtOpenKeyExFn>> = OnceLock::new();

    thread_local! {
        /// 当前线程是否正在执行“自引用 CreateProcess”——其间屏蔽对自身镜像的 IFEO 自检。
        static SUPPRESS_IFEO_SELF_CHECK: Cell<bool> = const { Cell::new(false) };
    }

    /// 进入自引用启动:置位 thread-local;Drop 时复位(异常路径也复位)。
    struct IfeoSelfCheckGuard;

    impl IfeoSelfCheckGuard {
        fn new() -> Self {
            SUPPRESS_IFEO_SELF_CHECK.with(|f| f.set(true));
            IfeoSelfCheckGuard
        }
    }

    impl Drop for IfeoSelfCheckGuard {
        fn drop(&mut self) {
            SUPPRESS_IFEO_SELF_CHECK.with(|f| f.set(false));
        }
    }

    /// 自身镜像 leaf 名(小写,如 `claude.exe`),IFEO 按 leaf 文件名匹配。
    fn self_image_basename() -> Option<&'static str> {
        static B: OnceLock<Option<String>> = OnceLock::new();
        B.get_or_init(|| {
            self_exe_path().map(|p| {
                p.rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(&p)
                    .to_ascii_lowercase()
            })
        })
        .as_deref()
    }

    /// 读 OBJECT_ATTRIBUTES.ObjectName(原样,不改大小写)。
    unsafe fn oa_object_name(object_attributes: *const ObjectAttributes) -> Option<String> {
        if object_attributes.is_null() {
            return None;
        }
        let name_ptr = (*object_attributes).object_name;
        if name_ptr.is_null() {
            return None;
        }
        let us = &*name_ptr;
        if us.buffer.is_null() || us.length == 0 {
            return None;
        }
        let len = (us.length / 2) as usize;
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            us.buffer, len,
        )))
    }

    /// name(小写)是否指向自身镜像的 IFEO 子键。
    fn name_is_self_ifeo(name_lower: &str) -> bool {
        let Some(base) = self_image_basename() else {
            return false;
        };
        // ntdll 既可能用绝对全路径一次打开(`…\image file execution options\claude.exe`),
        // 也可能先开 IFEO 基键、再以相对名(`claude.exe`)打开子键。两种都覆盖。
        name_lower == base
            || (name_lower.ends_with(&format!("\\{base}"))
                && name_lower.contains("image file execution options"))
    }

    /// 自引用启动期间,这次 NtOpenKey 是否在打开自身镜像的 IFEO 子键。
    unsafe fn is_self_ifeo_key_open(object_attributes: *const ObjectAttributes) -> bool {
        if !SUPPRESS_IFEO_SELF_CHECK.with(|f| f.get()) {
            return false;
        }
        oa_object_name(object_attributes)
            .map(|n| name_is_self_ifeo(&n.to_ascii_lowercase()))
            .unwrap_or(false)
    }

    /// 自引用启动期间,这次 NtQueryValueKey 是否在读 IFEO `Debugger` 值。
    unsafe fn is_self_ifeo_debugger_query(value_name: *const UnicodeString) -> bool {
        if !SUPPRESS_IFEO_SELF_CHECK.with(|f| f.get()) || value_name.is_null() {
            return false;
        }
        let us = &*value_name;
        if us.buffer.is_null() || us.length == 0 {
            return false;
        }
        let n = (us.length / 2) as usize;
        String::from_utf16_lossy(std::slice::from_raw_parts(us.buffer, n))
            .eq_ignore_ascii_case("Debugger")
    }

    // 自引用启动期间,堵住对自身镜像 IFEO 子键的“打开”——版本差异兜底:部分 Windows 上
    // CreateProcess 在这一步拿不到键就直接按“无 IFEO”启动。但实测 ARM64 Windows 上仅此不够
    // (系统会用缓存的 IFEO 基键句柄绕过本次打开后,再 NtQueryValueKey 读 `Debugger`),
    // 真正决定性的拦截点是下面的 NtQueryValueKey。两处都堵,跨版本稳妥。
    unsafe extern "system" fn hook_nt_open_key(
        key_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *const ObjectAttributes,
    ) -> i32 {
        if is_self_ifeo_key_open(object_attributes) {
            return STATUS_OBJECT_NAME_NOT_FOUND;
        }
        NT_OPEN_KEY.get().expect("NT_OPEN_KEY 未初始化").call(
            key_handle,
            desired_access,
            object_attributes,
        )
    }

    unsafe extern "system" fn hook_nt_open_key_ex(
        key_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *const ObjectAttributes,
        open_options: u32,
    ) -> i32 {
        if is_self_ifeo_key_open(object_attributes) {
            return STATUS_OBJECT_NAME_NOT_FOUND;
        }
        NT_OPEN_KEY_EX.get().expect("NT_OPEN_KEY_EX 未初始化").call(
            key_handle,
            desired_access,
            object_attributes,
            open_options,
        )
    }

    type NtQueryValueKeyFn = unsafe extern "system" fn(
        HANDLE,
        *const UnicodeString,
        u32,
        *mut c_void,
        u32,
        *mut u32,
    ) -> i32;
    static NT_QUERY_VALUE_KEY: OnceLock<GenericDetour<NtQueryValueKeyFn>> = OnceLock::new();

    // 决定性拦截点:自引用启动期间,IFEO `Debugger` 值的读取就是“把自己重定向到 shim”的源头。
    // 报告“值不存在”,系统按“无调试器”直接启动自身镜像(如 claude→claude.exe 的 ripgrep),不重定向。
    unsafe extern "system" fn hook_nt_query_value_key(
        key_handle: HANDLE,
        value_name: *const UnicodeString,
        info_class: u32,
        info: *mut c_void,
        length: u32,
        result_length: *mut u32,
    ) -> i32 {
        if is_self_ifeo_debugger_query(value_name) {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, Ordering::SeqCst) {
                log("已屏蔽自身镜像 IFEO Debugger 自检:自引用启动改为直接执行,不重定向到 shim");
            }
            if !result_length.is_null() {
                *result_length = 0;
            }
            return STATUS_OBJECT_NAME_NOT_FOUND;
        }
        NT_QUERY_VALUE_KEY
            .get()
            .expect("NT_QUERY_VALUE_KEY 未初始化")
            .call(
                key_handle,
                value_name,
                info_class,
                info,
                length,
                result_length,
            )
    }

    fn dll_path_wide() -> Option<Vec<u16>> {
        Some(
            dll_path()?
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect(),
        )
    }

    unsafe fn load_library_start() -> LPTHREAD_START_ROUTINE {
        let kernel32 = GetModuleHandleW(w!("kernel32.dll")).ok()?;
        let load_library = GetProcAddress(kernel32, s!("LoadLibraryW"))?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(load_library))
    }

    unsafe fn allocate_remote_path(process_handle: HANDLE, wide_path: &[u16]) -> *mut c_void {
        let byte_len = std::mem::size_of_val(wide_path);
        let remote_path = VirtualAllocEx(
            process_handle,
            None,
            byte_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote_path.is_null() {
            log("子进程注入失败:VirtualAllocEx 失败");
        }
        remote_path
    }

    unsafe fn write_remote_path(
        process_handle: HANDLE,
        remote_path: *mut c_void,
        wide_path: &[u16],
    ) -> bool {
        let byte_len = std::mem::size_of_val(wide_path);
        let result = WriteProcessMemory(
            process_handle,
            remote_path,
            wide_path.as_ptr().cast(),
            byte_len,
            None,
        );
        if let Err(e) = result {
            log(&format!("子进程注入失败:WriteProcessMemory 失败: {e}"));
            return false;
        }
        true
    }

    unsafe fn run_remote_load_library(
        process_handle: HANDLE,
        remote_path: *mut c_void,
        start: unsafe extern "system" fn(*mut c_void) -> u32,
    ) -> bool {
        let thread_result = CreateRemoteThread(
            process_handle,
            None,
            0,
            Some(start),
            Some(remote_path.cast_const()),
            0,
            None,
        );
        let thread_handle = match thread_result {
            Ok(handle) => handle,
            Err(e) => {
                log(&format!("子进程注入失败:CreateRemoteThread 失败: {e}"));
                return false;
            }
        };

        let wait_result = WaitForSingleObject(thread_handle, CHILD_INJECTION_TIMEOUT_MS);
        let mut exit_code = 0u32;
        if wait_result == WAIT_OBJECT_0 {
            let _ = GetExitCodeThread(thread_handle, &mut exit_code);
        }
        let _ = CloseHandle(thread_handle);
        if wait_result != WAIT_OBJECT_0 {
            log("子进程注入失败:LoadLibraryW 线程超时");
            return false;
        }
        if exit_code == 0 {
            log("子进程注入失败:LoadLibraryW 返回 0");
            return false;
        }
        true
    }

    unsafe fn inject_current_dll(process_handle: HANDLE) -> bool {
        let Some(wide_path) = dll_path_wide() else {
            log("子进程注入失败:无法定位 cphook.dll 路径");
            return false;
        };
        let Some(start) = load_library_start() else {
            log("子进程注入失败:无法定位 LoadLibraryW");
            return false;
        };
        let remote_path = allocate_remote_path(process_handle, &wide_path);
        if remote_path.is_null() {
            return false;
        }
        let written = write_remote_path(process_handle, remote_path, &wide_path);
        let loaded = written && run_remote_load_library(process_handle, remote_path, start);
        let _ = VirtualFreeEx(process_handle, remote_path, 0, MEM_RELEASE);
        loaded
    }

    unsafe fn wait_child_ready(pid: u32) -> bool {
        let name = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(pid));
        for _ in 0..CHILD_READY_OPEN_RETRIES {
            match OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) {
                Ok(event) => {
                    let ready = WaitForSingleObject(event, CHILD_READY_TIMEOUT_MS);
                    let _ = CloseHandle(event);
                    return ready == WAIT_OBJECT_0;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(40)),
            }
        }
        false
    }

    unsafe fn inject_created_child(
        pid: u32,
        process_handle: HANDLE,
        thread_handle: HANDLE,
        resume_after_injection: bool,
    ) -> bool {
        if !inject_current_dll(process_handle) {
            return false;
        }
        if !wait_child_ready(pid) {
            log(&format!("子进程注入失败:pid={pid} cphook 未就绪"));
            return false;
        }
        if resume_after_injection {
            let _ = ResumeThread(thread_handle);
        }
        log(&format!("子进程已注入并就绪 pid={pid}"));
        true
    }

    unsafe fn abort_created_child(
        process_information: *mut PROCESS_INFORMATION,
        api_name: &str,
    ) -> BOOL {
        let pi = &mut *process_information;
        let _ = TerminateProcess(pi.hProcess, 1);
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
        *pi = PROCESS_INFORMATION::default();
        SetLastError(ERROR_ACCESS_DENIED);
        log(&format!("{api_name} 子进程 hook 失败,已终止以防代理泄露"));
        BOOL(0)
    }

    unsafe fn finish_create_process_hook(
        process_information: *mut PROCESS_INFORMATION,
        original_flags: PROCESS_CREATION_FLAGS,
        api_name: &str,
    ) -> BOOL {
        if process_information.is_null() {
            return TRUE;
        }
        let pi = &mut *process_information;
        let resume_after_injection = !process_flags_are_suspended(original_flags);
        if inject_created_child(
            pi.dwProcessId,
            pi.hProcess,
            pi.hThread,
            resume_after_injection,
        ) {
            return TRUE;
        }
        abort_created_child(process_information, api_name)
    }

    unsafe extern "system" fn hook_create_process_w(
        application_name: PCWSTR,
        command_line: PWSTR,
        process_attributes: *const c_void,
        thread_attributes: *const c_void,
        inherit_handles: BOOL,
        creation_flags: PROCESS_CREATION_FLAGS,
        environment: *const c_void,
        current_directory: PCWSTR,
        startup_info: *const STARTUPINFOW,
        process_information: *mut PROCESS_INFORMATION,
    ) -> BOOL {
        let det = CREATE_PROCESS_W.get().expect("CREATE_PROCESS_W 未初始化");
        // 目标自引用内部工具(如 claude 用 claude.exe 当内置 ripgrep):原样放行、不接管,
        // 并在本次调用线程内屏蔽对自身镜像的 IFEO 自检——否则系统会按
        // `IFEO\<自身镜像>\Debugger` 把这次自引用启动重定向到 shim,而重定向丢掉 application_name,
        // 只把 `rg …` 交给 shim 当可执行启动而失败。屏蔽只在本进程本线程生效,外部启动 claude 仍被接管。
        if is_self_internal_tool_w(application_name, command_line) {
            let _ifeo_guard = IfeoSelfCheckGuard::new();
            return det.call(
                application_name,
                command_line,
                process_attributes,
                thread_attributes,
                inherit_handles,
                creation_flags,
                environment,
                current_directory,
                startup_info,
                process_information,
            );
        }
        let ret = det.call(
            application_name,
            command_line,
            process_attributes,
            thread_attributes,
            inherit_handles,
            process_flags_with_suspended(creation_flags),
            environment,
            current_directory,
            startup_info,
            process_information,
        );
        if !ret.as_bool() {
            return ret;
        }
        finish_create_process_hook(process_information, creation_flags, "CreateProcessW")
    }

    unsafe extern "system" fn hook_create_process_a(
        application_name: PCSTR,
        command_line: PSTR,
        process_attributes: *const c_void,
        thread_attributes: *const c_void,
        inherit_handles: BOOL,
        creation_flags: PROCESS_CREATION_FLAGS,
        environment: *const c_void,
        current_directory: PCSTR,
        startup_info: *const STARTUPINFOA,
        process_information: *mut PROCESS_INFORMATION,
    ) -> BOOL {
        let det = CREATE_PROCESS_A.get().expect("CREATE_PROCESS_A 未初始化");
        // 目标自引用内部工具:原样放行、不接管,并屏蔽自身镜像 IFEO 自检(同 CreateProcessW)。
        if is_self_internal_tool_a(application_name, command_line) {
            let _ifeo_guard = IfeoSelfCheckGuard::new();
            return det.call(
                application_name,
                command_line,
                process_attributes,
                thread_attributes,
                inherit_handles,
                creation_flags,
                environment,
                current_directory,
                startup_info,
                process_information,
            );
        }
        let ret = det.call(
            application_name,
            command_line,
            process_attributes,
            thread_attributes,
            inherit_handles,
            process_flags_with_suspended(creation_flags),
            environment,
            current_directory,
            startup_info,
            process_information,
        );
        if !ret.as_bool() {
            return ret;
        }
        finish_create_process_hook(process_information, creation_flags, "CreateProcessA")
    }

    unsafe extern "system" fn hook_get_tzi(out: *mut TIME_ZONE_INFORMATION) -> u32 {
        let det = GET_TZI.get().expect("GET_TZI 未初始化");
        let ret = det.call(out); // 先取真实值作基线(保留 DST 切换日期)
        if let Some(cfg) = config().as_ref() {
            if cfg.enabled && !out.is_null() {
                let z = &mut *out;
                z.Bias = cfg.tz_bias;
                z.StandardBias = 0;
                z.DaylightBias = -60;
                write_wide(&mut z.StandardName, &cfg.tz_std_name);
                write_wide(&mut z.DaylightName, &cfg.tz_dst_name);
            }
        }
        ret
    }

    unsafe extern "system" fn hook_get_dyn_tzi(out: *mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32 {
        let det = GET_DYN_TZI.get().expect("GET_DYN_TZI 未初始化");
        let ret = det.call(out);
        if let Some(cfg) = config().as_ref() {
            if cfg.enabled && !out.is_null() {
                let z = &mut *out;
                z.Bias = cfg.tz_bias;
                z.StandardBias = 0;
                z.DaylightBias = -60;
                write_wide(&mut z.StandardName, &cfg.tz_std_name);
                write_wide(&mut z.DaylightName, &cfg.tz_dst_name);
                // ICU 用 TimeZoneKeyName 映射到 IANA,是 Node Intl 时区的关键。
                write_wide(&mut z.TimeZoneKeyName, &cfg.tz_key_name);
                z.DynamicDaylightTimeDisabled = false.into();
            }
        }
        ret
    }

    unsafe fn fill_locale(det: &GenericDetour<GetLocaleFn>, buf: PWSTR, cch: i32) -> i32 {
        let ret = det.call(buf, cch);
        if let Some(cfg) = config().as_ref() {
            if cfg.enabled && !cfg.locale.is_empty() && !buf.is_null() && cch > 0 {
                let wide: Vec<u16> = cfg.locale.encode_utf16().collect();
                let n = wide.len().min((cch as usize).saturating_sub(1));
                let slice = std::slice::from_raw_parts_mut(buf.0, cch as usize);
                slice[..n].copy_from_slice(&wide[..n]);
                slice[n] = 0;
                return (n + 1) as i32;
            }
        }
        ret
    }

    unsafe extern "system" fn hook_get_user_locale(buf: PWSTR, cch: i32) -> i32 {
        fill_locale(
            GET_USER_LOCALE.get().expect("GET_USER_LOCALE 未初始化"),
            buf,
            cch,
        )
    }

    unsafe extern "system" fn hook_get_sys_locale(buf: PWSTR, cch: i32) -> i32 {
        fill_locale(
            GET_SYS_LOCALE.get().expect("GET_SYS_LOCALE 未初始化"),
            buf,
            cch,
        )
    }

    unsafe extern "system" fn hook_get_env_w(name: PCWSTR, buffer: PWSTR, size: u32) -> u32 {
        if read_pcwstr(name)
            .as_deref()
            .map(should_hide_env)
            .unwrap_or(false)
        {
            SetLastError(ERROR_ENVVAR_NOT_FOUND);
            return 0;
        }
        GET_ENV_W
            .get()
            .expect("GET_ENV_W 未初始化")
            .call(name, buffer, size)
    }

    unsafe extern "system" fn hook_get_env_a(name: PCSTR, buffer: PSTR, size: u32) -> u32 {
        if read_pcstr(name)
            .as_deref()
            .map(should_hide_env)
            .unwrap_or(false)
        {
            SetLastError(ERROR_ENVVAR_NOT_FOUND);
            return 0;
        }
        GET_ENV_A
            .get()
            .expect("GET_ENV_A 未初始化")
            .call(name, buffer, size)
    }

    unsafe extern "system" fn hook_connect(
        socket: SOCKET,
        name: *const SOCKADDR,
        name_len: i32,
    ) -> i32 {
        if should_block_udp(socket, name, name_len, "connect") {
            return -1;
        }
        let detour = CONNECT.get().expect("CONNECT 未初始化");
        if let Some(ret) = proxy_connect(socket, name, name_len, |relay, relay_len| {
            detour.call(socket, relay, relay_len)
        }) {
            return ret;
        }
        detour.call(socket, name, name_len)
    }

    unsafe extern "system" fn hook_send(
        socket: SOCKET,
        buffer: *const c_void,
        len: i32,
        flags: i32,
    ) -> i32 {
        // UDP 不在 send 拦截:连接态 UDP(QUIC)已在 connect 时按目的端口门控;
        // 这里只负责在首个 TCP 应用数据前补发私有头。
        if !ensure_proxy_header_sent(socket) {
            return -1;
        }
        SEND.get()
            .expect("SEND 未初始化")
            .call(socket, buffer, len, flags)
    }

    unsafe extern "system" fn hook_send_to(
        socket: SOCKET,
        buffer: *const c_void,
        len: i32,
        flags: i32,
        to: *const SOCKADDR,
        to_len: i32,
    ) -> i32 {
        if should_block_udp(socket, to, to_len, "sendto") {
            return -1;
        }
        SEND_TO
            .get()
            .expect("SEND_TO 未初始化")
            .call(socket, buffer, len, flags, to, to_len)
    }

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
        if should_block_udp(socket, to, to_len, "WSASendTo") {
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

    unsafe extern "system" fn hook_wsa_connect(
        socket: SOCKET,
        name: *const SOCKADDR,
        name_len: i32,
        caller_data: *const c_void,
        callee_data: *mut c_void,
        sqos: *mut c_void,
        gqos: *mut c_void,
    ) -> i32 {
        if should_block_udp(socket, name, name_len, "WSAConnect") {
            return -1;
        }
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

    unsafe extern "system" fn hook_connect_ex(
        socket: SOCKET,
        name: *const SOCKADDR,
        name_len: i32,
        send_buffer: *const c_void,
        send_data_len: u32,
        bytes_sent: *mut u32,
        overlapped: *mut c_void,
    ) -> BOOL {
        let runtime = proxy_runtime_config();
        // 是否改连 relay、以及是否透传(目标走本机已知代理端口):
        // - 外部目标:改连 relay,发私有头;
        // - 本机已知代理端口:改连 relay,透传(目标自己会发 HTTP CONNECT);
        // - 其它回环:直连放行。
        let target = sockaddr_target(name, name_len);
        let (should_redirect, passthrough) = match target {
            Some(t) if runtime.force_proxy_tcp && socket_is_tcp(socket) => {
                if is_loopback_ip(t.ip) {
                    if blocked_proxy_port(t.port) {
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
            // ConnectEx 携带初始数据 + overlapped 的异步形态难以在改连后保证“头/握手在数据前”,
            // 直接拒绝,迫使调用方回退到 connect + send(同样被 hook 接管)。
            if send_data_len > 0 && !overlapped.is_null() {
                WSASetLastError(WSAEOPNOTSUPP);
                return BOOL(0);
            }
            let target = target.expect("已校验目的地址有效");
            let Some(relay) = relay_sockaddr_for(name, name_len, runtime.relay_port) else {
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
                        std::slice::from_raw_parts(
                            send_buffer as *const u8,
                            send_data_len as usize,
                        ),
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

    unsafe extern "system" fn hook_close_socket(socket: SOCKET) -> i32 {
        forget_proxy_socket(socket);
        CLOSE_SOCKET
            .get()
            .expect("CLOSE_SOCKET 未初始化")
            .call(socket)
    }

    unsafe extern "system" fn hook_shutdown(socket: SOCKET, how: i32) -> i32 {
        forget_proxy_socket(socket);
        SHUTDOWN.get().expect("SHUTDOWN 未初始化").call(socket, how)
    }

    unsafe extern "system" fn hook_winhttp_get_ie_proxy_config(
        config: *mut WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    ) -> BOOL {
        if !config.is_null() {
            *config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG {
                fAutoDetect: false.into(),
                lpszAutoConfigUrl: PWSTR::null(),
                lpszProxy: PWSTR::null(),
                lpszProxyBypass: PWSTR::null(),
            };
            SetLastError(ERROR_SUCCESS);
            return TRUE;
        }
        WINHTTP_IE_PROXY
            .get()
            .expect("WINHTTP_IE_PROXY 未初始化")
            .call(config)
    }

    unsafe extern "system" fn hook_winhttp_get_proxy_for_url(
        session: *mut c_void,
        url: PCWSTR,
        auto_proxy_options: *const c_void,
        proxy_info: *mut WINHTTP_PROXY_INFO,
    ) -> BOOL {
        if !proxy_info.is_null() {
            *proxy_info = WINHTTP_PROXY_INFO {
                dwAccessType: WINHTTP_ACCESS_TYPE_NO_PROXY,
                lpszProxy: PWSTR::null(),
                lpszProxyBypass: PWSTR::null(),
            };
            SetLastError(ERROR_SUCCESS);
            return TRUE;
        }
        WINHTTP_PROXY_FOR_URL
            .get()
            .expect("WINHTTP_PROXY_FOR_URL 未初始化")
            .call(session, url, auto_proxy_options, proxy_info)
    }

    unsafe extern "system" fn hook_winhttp_open(
        user_agent: PCWSTR,
        _access_type: u32,
        _proxy_name: PCWSTR,
        _proxy_bypass: PCWSTR,
        flags: u32,
    ) -> *mut c_void {
        WINHTTP_OPEN.get().expect("WINHTTP_OPEN 未初始化").call(
            user_agent,
            WINHTTP_ACCESS_TYPE_NO_PROXY.0,
            PCWSTR::null(),
            PCWSTR::null(),
            flags,
        )
    }

    unsafe extern "system" fn hook_internet_query_option_w(
        internet: *mut c_void,
        option: u32,
        buffer: *mut c_void,
        buffer_len: *mut u32,
    ) -> BOOL {
        if option == INTERNET_OPTION_PER_CONNECTION_OPTION
            && !buffer.is_null()
            && !buffer_len.is_null()
        {
            let list = &mut *(buffer as *mut INTERNET_PER_CONN_OPTION_LISTW);
            if !list.pOptions.is_null() {
                let options =
                    std::slice::from_raw_parts_mut(list.pOptions, list.dwOptionCount as usize);
                for opt in options {
                    if opt.dwOption == INTERNET_PER_CONN_FLAGS {
                        opt.Value.dwValue = PROXY_TYPE_DIRECT;
                    }
                }
                SetLastError(ERROR_SUCCESS);
                return TRUE;
            }
        }
        INTERNET_QUERY_OPTION_W
            .get()
            .expect("INTERNET_QUERY_OPTION_W 未初始化")
            .call(internet, option, buffer, buffer_len)
    }

    unsafe fn proc_addr(module: PCWSTR, name: PCSTR, load_module: bool) -> Option<*const ()> {
        let h = match GetModuleHandleW(module) {
            Ok(h) => h,
            Err(_) if load_module => LoadLibraryW(module).ok()?,
            Err(_) => return None,
        };
        GetProcAddress(h, name).map(|f| f as *const ())
    }

    /// 安装一个 inline hook:解析地址 → GenericDetour → enable → 存入 static。
    unsafe fn install<F: retour::Function + Copy>(
        slot: &OnceLock<GenericDetour<F>>,
        module: PCWSTR,
        name: PCSTR,
        detour: F,
        label: &str,
        load_module: bool,
    ) -> bool {
        let Some(addr) = proc_addr(module, name, load_module) else {
            log(&format!("找不到 {label}"));
            return false;
        };
        let target: F = std::mem::transmute_copy(&addr);
        match GenericDetour::new(target, detour) {
            Ok(det) => match det.enable() {
                Ok(()) => {
                    let _ = slot.set(det);
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
            proc_addr(ws2, s!("socket"), true),
            proc_addr(ws2, s!("closesocket"), true),
            proc_addr(ws2, s!("WSAStartup"), true),
            proc_addr(ws2, s!("WSAIoctl"), true),
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
            Ok(det) => match det.enable() {
                Ok(()) => {
                    let _ = CONNECT_EX.set(det);
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

    unsafe extern "system" fn worker(_: *mut c_void) -> u32 {
        let Some(cfg) = config().as_ref() else {
            log("cphook 注入成功,但未读取到 hook.json");
            return 0;
        };
        log(&format!(
            "cphook 注入成功,enabled={} clear_proxy_env={} blocked_ports={:?}",
            cfg.enabled, cfg.clear_proxy_env, cfg.blocked_loopback_proxy_ports
        ));
        let mut ready = true;
        if cfg.enabled {
            ready &= install(
                &GET_TZI,
                w!("kernel32.dll"),
                s!("GetTimeZoneInformation"),
                hook_get_tzi as GetTziFn,
                "GetTimeZoneInformation",
                false,
            );
            ready &= install(
                &GET_DYN_TZI,
                w!("kernel32.dll"),
                s!("GetDynamicTimeZoneInformation"),
                hook_get_dyn_tzi as GetDynTziFn,
                "GetDynamicTimeZoneInformation",
                false,
            );
            ready &= install(
                &GET_USER_LOCALE,
                w!("kernel32.dll"),
                s!("GetUserDefaultLocaleName"),
                hook_get_user_locale as GetLocaleFn,
                "GetUserDefaultLocaleName",
                false,
            );
            ready &= install(
                &GET_SYS_LOCALE,
                w!("kernel32.dll"),
                s!("GetSystemDefaultLocaleName"),
                hook_get_sys_locale as GetLocaleFn,
                "GetSystemDefaultLocaleName",
                false,
            );
        }
        ready &= install(
            &GET_ENV_W,
            w!("kernel32.dll"),
            s!("GetEnvironmentVariableW"),
            hook_get_env_w as GetEnvWFn,
            "GetEnvironmentVariableW",
            false,
        );
        ready &= install(
            &GET_ENV_A,
            w!("kernel32.dll"),
            s!("GetEnvironmentVariableA"),
            hook_get_env_a as GetEnvAFn,
            "GetEnvironmentVariableA",
            false,
        );
        ready &= install(
            &CREATE_PROCESS_W,
            w!("kernel32.dll"),
            s!("CreateProcessW"),
            hook_create_process_w as CreateProcessWFn,
            "CreateProcessW",
            false,
        );
        ready &= install(
            &CREATE_PROCESS_A,
            w!("kernel32.dll"),
            s!("CreateProcessA"),
            hook_create_process_a as CreateProcessAFn,
            "CreateProcessA",
            false,
        );
        // IFEO 自检屏蔽(自引用启动用,见 hook_nt_open_key 注释):best-effort——
        // 失败仅回退到“自引用启动可能被 IFEO 重定向”的旧行为,不阻断注入(注入失败会终止目标)。
        let _ = install(
            &NT_OPEN_KEY,
            w!("ntdll.dll"),
            s!("NtOpenKey"),
            hook_nt_open_key as NtOpenKeyFn,
            "NtOpenKey",
            false,
        );
        let _ = install(
            &NT_OPEN_KEY_EX,
            w!("ntdll.dll"),
            s!("NtOpenKeyEx"),
            hook_nt_open_key_ex as NtOpenKeyExFn,
            "NtOpenKeyEx",
            false,
        );
        let _ = install(
            &NT_QUERY_VALUE_KEY,
            w!("ntdll.dll"),
            s!("NtQueryValueKey"),
            hook_nt_query_value_key as NtQueryValueKeyFn,
            "NtQueryValueKey",
            false,
        );
        ready &= install(
            &CONNECT,
            w!("Ws2_32.dll"),
            s!("connect"),
            hook_connect as ConnectFn,
            "connect",
            true,
        );
        ready &= install(
            &SEND,
            w!("Ws2_32.dll"),
            s!("send"),
            hook_send as SendFn,
            "send",
            true,
        );
        ready &= install(
            &SEND_TO,
            w!("Ws2_32.dll"),
            s!("sendto"),
            hook_send_to as SendToFn,
            "sendto",
            true,
        );
        ready &= install(
            &WSA_SEND,
            w!("Ws2_32.dll"),
            s!("WSASend"),
            hook_wsa_send as WsaSendFn,
            "WSASend",
            true,
        );
        ready &= install(
            &WSA_SEND_TO,
            w!("Ws2_32.dll"),
            s!("WSASendTo"),
            hook_wsa_send_to as WsaSendToFn,
            "WSASendTo",
            true,
        );
        ready &= install(
            &WSA_CONNECT,
            w!("Ws2_32.dll"),
            s!("WSAConnect"),
            hook_wsa_connect as WsaConnectFn,
            "WSAConnect",
            true,
        );
        ready &= install(
            &CLOSE_SOCKET,
            w!("Ws2_32.dll"),
            s!("closesocket"),
            hook_close_socket as CloseSocketFn,
            "closesocket",
            true,
        );
        ready &= install(
            &SHUTDOWN,
            w!("Ws2_32.dll"),
            s!("shutdown"),
            hook_shutdown as ShutdownFn,
            "shutdown",
            true,
        );
        // ConnectEx 用运行期解析指针 inline-hook(覆盖 Chromium/libuv 早已缓存的指针),
        // 而非 hook 导出符或拦截 WSAIoctl 查询。best-effort:失败不阻断非 ConnectEx 目标。
        let _ = install_connect_ex_hook();
        ready &= install(
            &WINHTTP_IE_PROXY,
            w!("Winhttp.dll"),
            s!("WinHttpGetIEProxyConfigForCurrentUser"),
            hook_winhttp_get_ie_proxy_config as WinHttpGetIeProxyConfigFn,
            "WinHttpGetIEProxyConfigForCurrentUser",
            true,
        );
        ready &= install(
            &WINHTTP_PROXY_FOR_URL,
            w!("Winhttp.dll"),
            s!("WinHttpGetProxyForUrl"),
            hook_winhttp_get_proxy_for_url as WinHttpGetProxyForUrlFn,
            "WinHttpGetProxyForUrl",
            true,
        );
        ready &= install(
            &WINHTTP_OPEN,
            w!("Winhttp.dll"),
            s!("WinHttpOpen"),
            hook_winhttp_open as WinHttpOpenFn,
            "WinHttpOpen",
            true,
        );
        ready &= install(
            &INTERNET_QUERY_OPTION_W,
            w!("Wininet.dll"),
            s!("InternetQueryOptionW"),
            hook_internet_query_option_w as InternetQueryOptionWFn,
            "InternetQueryOptionW",
            true,
        );
        if ready {
            signal_ready();
        } else {
            log("cphook 未就绪:关键 hook 未全部安装成功");
        }
        0
    }

    /// DLL 入口:仅记录模块句柄并起工作线程,绝不在 loader lock 下做重活。
    #[no_mangle]
    pub extern "system" fn DllMain(hinst: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
        if reason == DLL_PROCESS_ATTACH {
            HINST.store(hinst.0 as isize, Ordering::SeqCst);
            unsafe {
                let _ = CreateThread(None, 0, Some(worker), None, THREAD_CREATION_FLAGS(0), None);
            }
        }
        TRUE
    }

    #[cfg(test)]
    mod tests {
        use super::command_line_launches_self;

        #[test]
        fn self_exe_command_line_is_not_internal_tool() {
            assert!(command_line_launches_self(
                Some("\"C:\\tools\\target.exe\" --child"),
                "C:\\tools\\target.exe",
            ));
            assert!(command_line_launches_self(
                Some("target.exe --child"),
                "C:\\tools\\target.exe",
            ));
        }

        #[test]
        fn alternate_argv0_is_internal_tool_shape() {
            assert!(!command_line_launches_self(
                Some("rg --files"),
                "C:\\tools\\target.exe",
            ));
        }
    }
}
