use super::*;
use rustls::pki_types::{pem::PemObject, CertificateDer, UnixTime};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// 只推进测试客户端的验证时钟，不修改系统时间，也不更改证书校验算法。
#[derive(Debug)]
struct TestClock(AtomicU64);
impl rustls::time_provider::TimeProvider for TestClock {
    // 时间单位为 Unix 秒；测试始终使用正值，客户端每次握手读取最新值。
    fn current_time(&self) -> Option<UnixTime> {
        Some(UnixTime::since_unix_epoch(Duration::from_secs(
            self.0.load(Ordering::SeqCst),
        )))
    }
}

// 原客户端只加载一次公开 CA；禁用会话恢复，确保每次验收都实际校验服务器当前叶子证书。
fn trustedClient(pem: &str, now: OffsetDateTime) -> (Arc<rustls::ClientConfig>, Arc<TestClock>) {
    let mut roots = rustls::RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(pem.as_bytes()) {
        roots.add(certificate.unwrap()).unwrap();
    }
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let clock = Arc::new(TestClock(AtomicU64::new(now.unix_timestamp() as u64)));
    config.time_provider = clock.clone();
    config.resumption = rustls::client::Resumption::disabled();
    (Arc::new(config), clock)
}

// 通过真实 TLS 握手验证名称、有效期和签名；失败只返回布尔值，不打印证书对象。
async fn handshake(
    client: Arc<rustls::ClientConfig>,
    server: Arc<ServerConfig>,
    name: &'static str,
) -> bool {
    let (clientStream, serverStream) = tokio::io::duplex(8192);
    tokio::time::timeout(Duration::from_secs(3), async {
        let connector = tokio_rustls::TlsConnector::from(client);
        let acceptor = tokio_rustls::TlsAcceptor::from(server);
        let (client, server) = tokio::join!(
            connector.connect(name.try_into().unwrap(), clientStream),
            acceptor.accept(serverStream)
        );
        client.is_ok() && server.is_ok()
    })
    .await
    .expect("证书握手验收超时")
}

// 热路径持有的 ServerConfig Arc 不变，但解析出的证书更新；尚未达到续签阈值时不重复生成。
#[tokio::test]
async fn boundaryRotationPreservesOldTrustAndRefreshesAllHosts() {
    let now = OffsetDateTime::now_utc();
    let authority = Authority::signAt(
        &["chatgpt.com", "api.openai.com"],
        KeyPair::generate().unwrap(),
        now,
    )
    .unwrap();
    let (client, clock) = trustedClient(&authority.pem, now);
    let configs = authority.hosts.clone();
    let original = authority
        .signing
        .published
        .read()
        .unwrap()
        .certificates
        .clone();
    assert!(
        handshake(
            client.clone(),
            configs["chatgpt.com"].clone(),
            "chatgpt.com"
        )
        .await
    );
    let renewal = now + certificateLifetime - renewalMargin;
    assert!(!authority
        .signing
        .renew(renewal - time::Duration::seconds(1))
        .unwrap());
    assert!(Arc::ptr_eq(
        &original["chatgpt.com"],
        &authority.signing.published.read().unwrap().certificates["chatgpt.com"]
    ));
    assert!(authority.signing.renew(renewal).unwrap());
    assert!(!authority.signing.renew(renewal).unwrap());
    clock.0.store(
        (now + time::Duration::days(8)).unix_timestamp() as u64,
        Ordering::SeqCst,
    );
    for name in ["chatgpt.com", "api.openai.com"] {
        assert!(!Arc::ptr_eq(
            &original[name],
            &authority.signing.published.read().unwrap().certificates[name]
        ));
        assert!(Arc::ptr_eq(&configs[name], &authority.hosts[name]));
        // 第八天连最初公开 CA 的证书有效期也已过去；Rustls 保留的信任锚仍验证新叶子的签名和有效期。
        assert!(handshake(client.clone(), configs[name].clone(), name).await);
    }
    assert!(!handshake(client, configs["chatgpt.com"].clone(), "wrong.example").await);
}

// 旧证书在真实客户端时钟中确实过期失败；维护首轮立即续签，停用后任务退出且释放签名状态引用。
#[tokio::test]
async fn maintenanceRepairsExpiredBatchAndStops() {
    let now = OffsetDateTime::now_utc();
    let authority = Authority::signAt(
        &["chatgpt.com"],
        KeyPair::generate().unwrap(),
        now - time::Duration::days(8),
    )
    .unwrap();
    let (client, _) = trustedClient(&authority.pem, now);
    let server = authority.hosts["chatgpt.com"].clone();
    assert!(!handshake(client.clone(), server.clone(), "chatgpt.com").await);
    let cancel = CancellationToken::new();
    let worker = tokio::spawn(authority.maintenance(cancel.clone()));
    tokio::time::timeout(Duration::from_secs(3), async {
        while authority.signing.published.read().unwrap().renewAfter <= now {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("维护任务未及时续签");
    assert!(handshake(client, server, "chatgpt.com").await);
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Arc::strong_count(&authority.signing), 1);
}

// 已建立 TLS 流使用协商好的会话密钥，发布新叶子不能断开它或阻止后续字节往返。
#[tokio::test]
async fn establishedConnectionSurvivesRenewal() {
    let now = OffsetDateTime::now_utc();
    let authority = Authority::signAt(&["chatgpt.com"], KeyPair::generate().unwrap(), now).unwrap();
    let (clientConfig, _) = trustedClient(&authority.pem, now);
    let connector = tokio_rustls::TlsConnector::from(clientConfig);
    let acceptor = tokio_rustls::TlsAcceptor::from(authority.hosts["chatgpt.com"].clone());
    let (clientStream, serverStream) = tokio::io::duplex(8192);
    let (client, server) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            connector.connect("chatgpt.com".try_into().unwrap(), clientStream),
            acceptor.accept(serverStream)
        )
    })
    .await
    .expect("原连接握手超时");
    let (mut client, mut server) = (client.unwrap(), server.unwrap());
    assert!(authority.signing.renew(now + certificateLifetime).unwrap());
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut received = [0u8; 4];
        let (write, read) =
            tokio::join!(client.write_all(b"test"), server.read_exact(&mut received));
        write.unwrap();
        read.unwrap();
        assert_eq!(&received, b"test");
        let (write, read) =
            tokio::join!(server.write_all(b"done"), client.read_exact(&mut received));
        write.unwrap();
        read.unwrap();
        assert_eq!(&received, b"done");
    })
    .await
    .expect("续签后原连接字节往返失败");
}

// 已取消的维护任务连首轮都不签发，避免停用操作之后产生新的证书批次。
#[tokio::test]
async fn preCancelledMaintenanceDoesNotRenew() {
    let now = OffsetDateTime::now_utc();
    let authority = Authority::signAt(
        &["chatgpt.com"],
        KeyPair::generate().unwrap(),
        now - time::Duration::days(8),
    )
    .unwrap();
    let before = authority.signing.published.read().unwrap().renewAfter;
    let cancel = CancellationToken::new();
    cancel.cancel();
    authority.maintenance(cancel).await;
    assert_eq!(
        before,
        authority.signing.published.read().unwrap().renewAfter
    );
}

// 第二个站点签发失败时，第一个站点生成的临时证书也不发布；旧批次保持完整且错误明确返回。
#[test]
fn partialSigningFailureNeverPublishesBatch() {
    let now = OffsetDateTime::now_utc();
    let mut authority =
        Authority::signAt(&["chatgpt.com"], KeyPair::generate().unwrap(), now).unwrap();
    let previous = authority.signing.published.read().unwrap().certificates["chatgpt.com"].clone();
    Arc::get_mut(&mut authority.signing)
        .unwrap()
        .names
        .push("非ASCII.example".into());
    assert_eq!(
        authority
            .signing
            .renew(now + certificateLifetime)
            .unwrap_err(),
        "观测主机名无效"
    );
    let batch = authority.signing.published.read().unwrap();
    assert_eq!(batch.certificates.len(), 1);
    assert!(Arc::ptr_eq(&previous, &batch.certificates["chatgpt.com"]));
    assert_eq!(batch.renewAfter, now + certificateLifetime - renewalMargin);
}

// 并发维护或系统时钟回拨导致较旧请求晚到时，既有新批次不可被重新签成较旧有效期。
#[test]
fn earlierClockDoesNotReplaceNewerBatch() {
    let now = OffsetDateTime::now_utc();
    let authority = Authority::signAt(&["chatgpt.com"], KeyPair::generate().unwrap(), now).unwrap();
    let later = now + time::Duration::days(20);
    assert!(authority.signing.renew(later).unwrap());
    let previous = authority.signing.published.read().unwrap().certificates["chatgpt.com"].clone();
    assert!(!authority
        .signing
        .renew(now + time::Duration::days(10))
        .unwrap());
    assert!(Arc::ptr_eq(
        &previous,
        &authority.signing.published.read().unwrap().certificates["chatgpt.com"]
    ));
}
