//! 固定白名单的观测证书：签名身份跨重启复用，叶子在到期前整批续签，既有连接不重建。
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rustls::{
    pki_types::PrivatePkcs8KeyDer,
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
    ServerConfig,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

// 主体名称与公钥构成客户端的信任锚；续签只替换叶子，不轮换已发布的信任身份或修改系统根集合。
const issuerCommonName: &str = "本地直连观测临时证书";
const certificateLifetime: time::Duration = time::Duration::days(7);
const renewalMargin: time::Duration = time::Duration::days(1);
const clockTolerance: time::Duration = time::Duration::minutes(5);
const renewalInterval: Duration = Duration::from_secs(3600);

// 所有白名单证书作为同一批次发布，避免部分主机更新后其他主机仍使用过期证书。
struct CertificateBatch {
    renewAfter: OffsetDateTime,
    certificates: HashMap<String, Arc<CertifiedKey>>,
}

// 签发材料仅在运行期内存中使用；共享状态不实现 Debug，防止诊断意外输出密钥或证书内容。
struct SigningState {
    key: KeyPair,
    issuer: Certificate,
    names: Vec<String>,
    published: Arc<RwLock<CertificateBatch>>,
}

// 每个固定主机的 TLS 配置保持稳定，握手时只复制当前证书 Arc，不在网络热路径生成密钥。
struct SiteResolver {
    host: String,
    published: Arc<RwLock<CertificateBatch>>,
}

impl std::fmt::Debug for SiteResolver {
    // Rustls 要求解析器可诊断；仅输出类型名，不展开共享签名材料。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SiteResolver")
    }
}

impl ResolvesServerCert for SiteResolver {
    // 状态锁损坏时让当前 TLS 握手失败并记录静态错误，不以未观察到的隧道透传掩盖证书状态异常。
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        match self.published.read() {
            Ok(batch) => batch.certificates.get(&self.host).cloned(),
            Err(_) => {
                log::error!("观测证书状态锁损坏");
                None
            }
        }
    }
}

pub(super) struct Authority {
    pub pem: String,
    pub hosts: HashMap<String, Arc<ServerConfig>>,
    signing: Arc<SigningState>,
}

impl Authority {
    // Windows 从当前用户 DPAPI 保护文件恢复同一签名身份；其他平台只保留显式代理的临时身份。
    pub fn forRuntime(hosts: &[&str], directory: &std::path::Path) -> Result<Self, String> {
        #[cfg(windows)]
        {
            Self::signAt(
                hosts,
                super::authorityStore::loadOrCreate(directory)?,
                OffsetDateTime::now_utc(),
            )
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            Self::create(hosts)
        }
    }

    // 隔离测试与非 Windows 显式代理使用独立密钥；不接触正式签名身份。
    #[cfg(any(test, not(windows)))]
    pub fn create(hosts: &[&str]) -> Result<Self, String> {
        let key = KeyPair::generate().map_err(|_| "生成观测签名密钥失败")?;
        Self::signAt(hosts, key, OffsetDateTime::now_utc())
    }

    // 以明确时间签发首批证书；测试可推进独立时钟，生产始终传入当前 UTC，不改操作系统时间。
    pub(super) fn signAt(
        hosts: &[&str],
        key: KeyPair,
        now: OffsetDateTime,
    ) -> Result<Self, String> {
        let mut params = CertificateParams::default();
        params.not_before = now - clockTolerance;
        params.not_after = now + certificateLifetime;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, issuerCommonName);
        let issuer = params.self_signed(&key).map_err(|_| "签发观测根证书失败")?;
        let names: Vec<_> = hosts.iter().map(|host| (*host).to_owned()).collect();
        let batch = signBatch(&names, &key, &issuer, now)?;
        let published = Arc::new(RwLock::new(batch));
        let mut configurations = HashMap::new();
        for host in &names {
            let mut server = ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| "初始化观测 TLS 版本失败")?
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(SiteResolver {
                host: host.clone(),
                published: published.clone(),
            }));
            server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            configurations.insert(host.clone(), Arc::new(server));
        }
        Ok(Self {
            pem: issuer.pem(),
            hosts: configurations,
            signing: Arc::new(SigningState {
                key,
                issuer,
                names,
                published,
            }),
        })
    }

    // 返回拥有签名状态的独立任务，由连接 TaskTracker 管理；停用优先，不留下无宿主的续签循环。
    pub fn maintenance(
        &self,
        cancel: CancellationToken,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        let signing = self.signing.clone();
        async move {
            let mut timer = tokio::time::interval(renewalInterval);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased; _ = cancel.cancelled() => break, _ = timer.tick() => {} }
                let signing = signing.clone();
                // 生成站点密钥是同步工作；不阻塞 TLS/SSE 的 Tokio 工作线程，不持有发布锁进行签发。
                match tokio::task::spawn_blocking(move || signing.renew(OffsetDateTime::now_utc()))
                    .await
                {
                    Ok(Ok(true)) => log::info!("观测站点证书已续签"),
                    Ok(Ok(false)) => {}
                    Ok(Err(error)) => log::error!("观测证书续签失败：{error}"),
                    Err(error) => log::error!("观测证书续签任务异常退出：{error}"),
                }
            }
        }
    }
}

impl SigningState {
    // 到期前一天续签；完整批次成功才原子发布，失败保留现有批次并由维护任务显式报告和重试。
    fn renew(&self, now: OffsetDateTime) -> Result<bool, String> {
        if now
            < self
                .published
                .read()
                .map_err(|_| "读取观测证书状态失败")?
                .renewAfter
        {
            return Ok(false);
        }
        let batch = signBatch(&self.names, &self.key, &self.issuer, now)?;
        let mut published = self.published.write().map_err(|_| "发布观测证书状态失败")?;
        // 并发签发或时钟回拨不能覆盖已经较新的批次。
        if now < published.renewAfter {
            return Ok(false);
        }
        *published = batch;
        Ok(true)
    }
}

// 签发固定主机集合并校验公私钥匹配；任一站点失败整批返回错误，不发布不完整集合。
fn signBatch(
    names: &[String],
    key: &KeyPair,
    issuer: &Certificate,
    now: OffsetDateTime,
) -> Result<CertificateBatch, String> {
    let mut certificates = HashMap::new();
    for host in names {
        let leafKey = KeyPair::generate().map_err(|_| "生成观测站点密钥失败")?;
        let mut leaf = CertificateParams::new(vec![host.clone()]).map_err(|_| "观测主机名无效")?;
        leaf.not_before = now - clockTolerance;
        leaf.not_after = now + certificateLifetime;
        let certificate = leaf
            .signed_by(&leafKey, issuer, key)
            .map_err(|_| "签发观测站点证书失败")?;
        let certified = CertifiedKey::from_der(
            vec![certificate.der().clone()],
            PrivatePkcs8KeyDer::from(leafKey.serialize_der()).into(),
            &rustls::crypto::ring::default_provider(),
        )
        .map_err(|_| "初始化观测证书链失败")?;
        certificates.insert(host.clone(), Arc::new(certified));
    }
    Ok(CertificateBatch {
        certificates,
        renewAfter: now + certificateLifetime - renewalMargin,
    })
}

#[cfg(test)]
#[path = "../../tests/observation/certificateRenewalTests.rs"]
mod tests;
