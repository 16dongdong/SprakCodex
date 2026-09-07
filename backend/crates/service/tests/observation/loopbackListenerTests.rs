use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// 使用真实 socket 验证两个回环地址共享端口、数据一致，释放后不再接受任何地址族的新连接。
#[tokio::test]
async fn bothFamiliesSharePortAndCloseTogether() {
    let listeners = LoopbackListeners::bind().await.unwrap();
    let port = listeners.port();
    for address in [
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
        std::net::IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        let (sender, receiver) =
            tokio::join!(TcpStream::connect((address, port)), listeners.accept());
        let mut sender = sender.unwrap();
        let mut receiver = receiver.unwrap();
        assert_eq!(receiver.local_addr().unwrap().ip(), address);
        sender.write_all(b"OK").await.unwrap();
        let mut received = [0; 2];
        receiver.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"OK");
    }
    drop(listeners);
    for address in [
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
        std::net::IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        assert!(TcpStream::connect((address, port)).await.is_err());
    }
}
