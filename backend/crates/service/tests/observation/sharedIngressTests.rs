use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// 单端口上交错发送分片原生头与普通 HTTP；分流必须保留所有字节，订阅退出后私有入口关闭。
#[tokio::test]
async fn samePortPreservesBothProtocols() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    publish(port).unwrap();
    let (_, mut observed) = subscribe().unwrap().unwrap();
    let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let (server, _) = listener.accept().await.unwrap();
    let routing = tokio::spawn(route(server));
    client.write_all(b"CPRO").await.unwrap();
    tokio::task::yield_now().await;
    client.write_all(b"XYH1").await.unwrap();
    assert!(routing.await.unwrap().unwrap().is_none());
    let mut server = TcpStream::from_std(observed.recv().await.unwrap()).unwrap();
    let mut header = [0u8; 8];
    server.read_exact(&mut header).await.unwrap();
    assert_eq!(&header, b"CPROXYH1");
    let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let (server, _) = listener.accept().await.unwrap();
    client
        .write_all(b"GET /health HTTP/1.1\r\n\r\n")
        .await
        .unwrap();
    let mut server = route(server).await.unwrap().unwrap();
    let mut prefix = [0u8; 3];
    server.read_exact(&mut prefix).await.unwrap();
    assert_eq!(&prefix, b"GET");
    drop(observed);
}
