//! 固定白名单的观测证书：Windows 运行期复用用户范围保护的签名身份，测试可生成隔离临时 CA。
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use rustls::{pki_types::PrivatePkcs8KeyDer, ServerConfig};
use std::{collections::HashMap, sync::Arc};

// 主体名称与公钥共同组成既有客户端的信任锚，属于持久化协议，升级时保持不变。
const issuerCommonName: &str = "本地直连观测临时证书";

pub(super) struct Authority {
    pub pem: String,
    pub hosts: HashMap<String, Arc<ServerConfig>>,
}

impl Authority {
    // 运行期复用同一签名密钥，重启后新叶子仍可由旧客户端的信任锚验证；其他平台沿用显式临时代理能力。
    pub fn forRuntime(hosts: &[&str], directory: &std::path::Path) -> Result<Self, String> {
        #[cfg(windows)]
        {
            Self::sign(hosts, super::authorityStore::loadOrCreate(directory)?)
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            Self::create(hosts)
        }
    }

    // 预先签发固定白名单，TLS 热路径不生成密钥；主机名与证书链失败直接返回启动错误。
    #[cfg(any(test, not(windows)))]
    pub fn create(hosts: &[&str]) -> Result<Self, String> {
        let signingKey = KeyPair::generate().map_err(|_| "生成观测签名密钥失败")?;
        Self::sign(hosts, signingKey)
    }

    // 同一签名身份签发本次运行的短期叶子证书，私钥不作为响应字段或诊断输出。
    fn sign(hosts: &[&str], signingKey: KeyPair) -> Result<Self, String> {
        let now = time::OffsetDateTime::now_utc();
        let mut params = CertificateParams::default();
        params.not_before = now - time::Duration::minutes(5);
        params.not_after = now + time::Duration::days(7);
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, issuerCommonName);
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
