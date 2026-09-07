//! 每次启用生成独立 CA，私钥只在内存；仅导出公开证书供新启动的客户端按进程信任。
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use rustls::{pki_types::PrivatePkcs8KeyDer, ServerConfig};
use std::{collections::HashMap, sync::Arc};

pub(super) struct Authority {
    pub pem: String,
    pub hosts: HashMap<String, Arc<ServerConfig>>,
}

impl Authority {
    // 预先签发固定白名单，TLS 热路径不生成密钥；主机名与证书链失败直接返回启动错误。
    pub fn create(hosts: &[&str]) -> Result<Self, String> {
        let now = time::OffsetDateTime::now_utc();
        let mut params = CertificateParams::default();
        params.not_before = now - time::Duration::minutes(5);
        params.not_after = now + time::Duration::days(7);
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "本地直连观测临时证书");
        let signingKey = KeyPair::generate().map_err(|_| "生成观测签名密钥失败")?;
        let issuer = params
            .self_signed(&signingKey)
            .map_err(|_| "签发观测根证书失败")?;
        let mut configurations = HashMap::new();
        for host in hosts {
            let key = KeyPair::generate().map_err(|_| "生成观测站点密钥失败")?;
            let mut leaf =
                CertificateParams::new(vec![(*host).into()]).map_err(|_| "观测主机名无效")?;
            leaf.not_before = now - time::Duration::minutes(5);
            leaf.not_after = now + time::Duration::days(7);
            let certificate = leaf
                .signed_by(&key, &issuer, &signingKey)
                .map_err(|_| "签发观测站点证书失败")?;
            let mut server = ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| "初始化观测 TLS 版本失败")?
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.der().clone()],
                PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
            )
            .map_err(|_| "初始化观测证书链失败")?;
            server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            configurations.insert((*host).into(), Arc::new(server));
        }
        Ok(Self {
            pem: issuer.pem(),
            hosts: configurations,
        })
    }
}
