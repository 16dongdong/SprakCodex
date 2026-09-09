use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

// 只关闭选中的真实 TCP 连接；原始句柄仍由客户端回收，其余 IPC 连接可以继续传输。
#[test]
fn reconnectOnlySelectedPeer() {
    let selected = TcpListener::bind("127.0.0.1:0").unwrap();
    let preserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(selected.local_addr().unwrap()).unwrap();
    let (mut server, _) = selected.accept().unwrap();
    let mut other = TcpStream::connect(preserved.local_addr().unwrap()).unwrap();
    let (mut otherServer, _) = preserved.accept().unwrap();
    assert_eq!(
        reconnectWhere(|peer| peer == selected.local_addr().unwrap()).unwrap(),
        1
    );
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(server.read(&mut [0u8; 1]).unwrap(), 0);
    other.write_all(b"x").unwrap();
    let mut received = [0u8; 1];
    otherServer.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"x");
    drop(client);
}
