use super::*;
use crate::directObservation::certificateAuthority::Authority;
use std::{path::PathBuf, sync::Arc, time::Duration};

// 密钥测试只创建独立目录；所有私钥都是测试时生成，析构严格删除自己的普通文件。
struct Fixture {
    directory: PathBuf,
}
impl Fixture {
    // 不使用正式数据库目录或系统证书库，DPAPI 仅用于当前测试用户的临时密文。
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "observationAuthority{:032x}",
            rand::random::<u128>()
        ));
        std::fs::create_dir(&directory).unwrap();
        Self { directory }
    }
}
impl Drop for Fixture {
    // 锁文件在 loadOrCreate 返回后已关闭；清理失败直接使测试失败，不隐藏文件占用问题。
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.directory).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::remove_file(entry.path()).unwrap();
        }
        std::fs::remove_dir(&self.directory).unwrap();
    }
}

// 跨两次加载的公钥身份一致，磁盘内容不能作为 PKCS#8 明文使用。
#[test]
fn protectedIdentityIsReused() {
    let fixture = Fixture::new();
    let first = loadOrCreate(&fixture.directory).unwrap();
    let second = loadOrCreate(&fixture.directory).unwrap();
    assert!(first.public_key_der() == second.public_key_der());
    let protected = std::fs::read(fixture.directory.join(protectedKeyName)).unwrap();
    let der = rustls::pki_types::PrivatePkcs8KeyDer::from(protected.as_slice());
    assert!(KeyPair::try_from(&der).is_err());
}

// 密文损坏必须显式失败，不自动轮换根密钥造成所有旧客户端失去信任。
#[test]
fn corruptIdentityDoesNotRotate() {
    let fixture = Fixture::new();
    loadOrCreate(&fixture.directory).unwrap();
    let path = fixture.directory.join(protectedKeyName);
    std::fs::write(&path, b"corrupt fixture").unwrap();
    assert!(loadOrCreate(&fixture.directory).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"corrupt fixture");
}

// 仅把旧公开证书加入客户端根集合，模拟在 Manager 重启前已经创建的 TLS 客户端。
fn trustedClient(pem: &str) -> Arc<rustls::ClientConfig> {
    use rustls::pki_types::pem::PemObject;
    let mut roots = rustls::RootCertStore::empty();
    for certificate in rustls::pki_types::CertificateDer::pem_slice_iter(pem.as_bytes()) {
        roots.add(certificate.unwrap()).unwrap();
    }
    Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth(),
    )
}

// 在真正的 TLS 握手中验证新叶子与旧信任锚，返回布尔值避免失败报告打印证书或密钥对象。
async fn handshake(client: Arc<rustls::ClientConfig>, server: Arc<rustls::ServerConfig>) -> bool {
    let (clientStream, serverStream) = tokio::io::duplex(8192);
    let connector = tokio_rustls::TlsConnector::from(client);
    let acceptor = tokio_rustls::TlsAcceptor::from(server);
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, server) = tokio::join!(
            connector.connect("chatgpt.com".try_into().unwrap(), clientStream),
            acceptor.accept(serverStream)
        );
        client.is_ok() && server.is_ok()
    })
    .await
    .expect("TLS 验收超时")
}

// 销毁原 Authority 后重新从 DPAPI 密文加载，原客户端仍应信任新签发的站点证书。
#[tokio::test]
async fn oldClientTrustSurvivesAuthorityRecreation() {
    let fixture = Fixture::new();
    let first = Authority::forRuntime(&["chatgpt.com"], &fixture.directory).unwrap();
    let client = trustedClient(&first.pem);
    drop(first);
    let second = Authority::forRuntime(&["chatgpt.com"], &fixture.directory).unwrap();
    assert!(handshake(client, second.hosts["chatgpt.com"].clone()).await);
}

// 同名但不同签名身份的证书仍必须拒绝，证明恢复并未依赖关闭证书校验。
#[tokio::test]
async fn differentSigningIdentityIsStillRejected() {
    let fixture = Fixture::new();
    let first = Authority::forRuntime(&["chatgpt.com"], &fixture.directory).unwrap();
    let other = Authority::create(&["chatgpt.com"]).unwrap();
    assert!(
        !handshake(
            trustedClient(&first.pem),
            other.hosts["chatgpt.com"].clone()
        )
        .await
    );
}
