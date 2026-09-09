use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::AsRawSocket;

// 并发调用真实 Winsock send trampoline，确认私有头仅出现一次；测试不安装全局系统 hook。
#[test]
fn concurrentSendWritesRelayHeaderOnce() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut reader, _) = listener.accept().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let socket = SOCKET(sender.as_raw_socket() as usize);
    unsafe {
        let module = GetModuleHandleW(w!("Ws2_32.dll")).unwrap();
        let send: SendFn = std::mem::transmute(GetProcAddress(module, s!("send")).unwrap());
        // 只创建原调用 trampoline，不 enable，不影响测试程序的其他网络调用。
        SEND.set(RawDetour::new(send as *const (), hook_send as *const ()).unwrap())
            .unwrap();
    }
    let target = HookProxyTarget {
        ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
        port: 443,
        pid: std::process::id(),
    };
    remember_proxy_socket(socket, target, false);
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(move || assert!(unsafe { ensure_proxy_header_sent(socket) }));
        }
    });
    sender.write_all(b"END").unwrap();
    sender.shutdown(std::net::Shutdown::Write).unwrap();
    let mut received = Vec::new();
    reader.read_to_end(&mut received).unwrap();
    let mut expected = cpcommon::hook_proxy::encode_header(&target).to_vec();
    expected.extend_from_slice(b"END");
    assert_eq!(received, expected);
    forget_proxy_socket(socket);
    assert!(!proxy_sockets()
        .lock()
        .unwrap()
        .contains_key(&socket_key(socket)));
}
