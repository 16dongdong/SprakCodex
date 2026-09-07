use super::*;
use cpcommon::hook_proxy::{encodeRoute, HookProxyTarget, RouteKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// 本地原出口只处理固定测试 HTTP，不访问外网；同时返回实际收到的请求行作为路由证据。
async fn upstream(
    listener: tokio::net::TcpListener,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<String>,
) {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 1024];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0 && bytes.len() + count <= 16384);
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
        }
        sender
            .send(
                String::from_utf8(bytes)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .await
            .unwrap();
    });
    (worker, receiver)
}

// 传输测试不创建数据库，路径不属于推理接口，因此不会产生待接收的计量记录。
async fn engine(
    proxy: Option<String>,
) -> (
    Arc<Engine>,
    tokio::sync::mpsc::Receiver<super::super::recordSink::Record>,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let sink = RecordSink {
        sender,
        counters: Arc::new(super::super::recordSink::Counters::default()),
    };
    let engine = Engine::new(
        Authority::create(&["chatgpt.com"]).unwrap(),
        sink,
        proxy,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    (Arc::new(engine), receiver)
}

// 宿主未配置或配置了另一出口时，原生代理头都必须选中客户端的原代理，不能靠测试手动填写宿主代理才成功。
#[tokio::test]
async fn nativeProxyMetadataOverridesHostProxyChoice() {
    for configured in [false, true] {
        let original = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = original.local_addr().unwrap();
        let wrong = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (worker, received) = upstream(original).await;
        let (engine, _records) =
            engine(configured.then(|| format!("http://{}", wrong.local_addr().unwrap()))).await;
        let listener = LoopbackListeners::bind().await.unwrap();
        let relay = std::net::SocketAddr::from(([127, 0, 0, 1], listener.port()));
        let cancel = engine.cancel.clone();
        let serving = tokio::spawn(serve(listener, engine));
        let mut socket = TcpStream::connect(relay).await.unwrap();
        socket
            .write_all(&encodeRoute(
                &HookProxyTarget {
                    ip: address.ip(),
                    port: address.port(),
                    pid: std::process::id(),
                },
                RouteKind::HttpProxy,
            ))
            .await
            .unwrap();
        socket.write_all(b"GET http://fixture.invalid/probe HTTP/1.1\r\nHost: fixture.invalid\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !response.ends_with(b"\r\n\r\nOK") {
                let mut buffer = [0; 1024];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                response.extend_from_slice(&buffer[..count]);
            }
        })
        .await
        .expect("原代理请求未完成");
        assert!(received
            .await
            .unwrap()
            .starts_with("GET http://fixture.invalid/probe HTTP/1.1"));
        assert!(
            tokio::time::timeout(Duration::from_millis(30), wrong.accept())
                .await
                .is_err()
        );
        drop(socket);
        cancel.cancel();
        serving.await.unwrap();
        worker.await.unwrap();
    }
}

// 直连来源固定原 IP，HTTP 与 WebSocket 的 TCP 入口都不再重新选择宿主代理或解析不同地址。
#[tokio::test]
async fn directPinKeepsOriginalDestination() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (worker, received) = upstream(listener).await;
    let (root, _records) = engine(Some("http://127.0.0.1:1".into())).await;
    let routed = root
        .forNativeRoute(None, Some(("fixture.invalid".into(), address)))
        .unwrap();
    assert!(routed.proxy.is_none());
    let response = routed
        .client
        .get(format!("http://fixture.invalid:{}/probe", address.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "OK");
    assert!(received.await.unwrap().starts_with("GET /probe HTTP/1.1"));
    worker.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let routed = root
        .forNativeRoute(None, Some(("fixture.invalid".into(), address)))
        .unwrap();
    let (socket, accepted) = tokio::join!(
        routed.connect("fixture.invalid", address.port()),
        listener.accept()
    );
    assert_eq!(socket.unwrap().peer_addr().unwrap(), address);
    assert!(accepted.is_ok());
}
