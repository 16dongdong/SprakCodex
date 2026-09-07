//! 直连观测宿主：线程、TLS 密钥与数据库消费者均在当前进程，不依赖独立程序。
//! 开关默认关闭且不跨重启恢复；临时证书只通过子进程环境传递，不安装系统根证书。
mod certificateAuthority;
mod recordSink;
mod streamObserver;
mod transport;
mod usageParser;
mod websocketRelay;

use recordSink::Counters;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex, OnceLock},
};
use tokio_util::sync::CancellationToken;

const observationHosts: &[&str] = &["chatgpt.com", "api.openai.com"];
pub const enabledSettingKey: &str = "directObservation.enabled";
static runningEngine: OnceLock<Mutex<Option<Running>>> = OnceLock::new();

struct Running {
    address: String,
    certificate: PathBuf,
    cancel: CancellationToken,
    thread: std::thread::JoinHandle<()>,
    counters: Arc<Counters>,
}

#[derive(Serialize)]
pub struct ObservationStatus {
    pub running: bool,
    pub proxyUrl: Option<String>,
    pub certificatePath: Option<String>,
    pub writtenRequests: u64,
    pub storageErrors: u64,
}

// 读取当前进程的观测状态；锁损坏返回显式错误，不报告虚假的关闭或启动成功。
pub fn status() -> Result<ObservationStatus, String> {
    let guard = runningEngine
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "观测状态锁损坏")?;
    Ok(statusOf(guard.as_ref()))
}

// 状态只包含公开证书路径与计数，既不返回业务认证字段，也不暴露网络出口密码。
fn statusOf(current: Option<&Running>) -> ObservationStatus {
    ObservationStatus {
        running: current.is_some_and(|engine| !engine.thread.is_finished()),
        proxyUrl: current.map(|engine| engine.address.clone()),
        certificatePath: current.map(|engine| engine.certificate.to_string_lossy().into_owned()),
        writtenRequests: current
            .map_or(0, |engine| engine.counters.written.load(Ordering::Relaxed)),
        storageErrors: current.map_or(0, |engine| engine.counters.errors.load(Ordering::Relaxed)),
    }
}

// 管理员显式启动后才签发公开证书；先确认数据库、绑定端口，再发布可用状态。
// 出口继承现有全局网络代理配置，但不访问账号池或改写任何认证请求。
pub fn start() -> Result<ObservationStatus, String> {
    let mut guard = runningEngine
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "观测状态锁损坏")?;
    if let Some(current) = guard.as_ref() {
        if current.thread.is_finished() {
            return Err("观测线程已退出，请先关闭后重新启用".into());
        }
        return Ok(statusOf(Some(current)));
    }
    crate::storage_helpers::initialize_storage()?;
    let authority = certificateAuthority::Authority::create(observationHosts)?;
    let folder = crate::process_env::db_dir();
    let certificate = folder.join(format!(
        "observationCertificate{:032x}.pem",
        rand::random::<u128>()
    ));
    std::fs::write(&certificate, &authority.pem).map_err(|_| "保存观测公开证书失败")?;
    let result = startRuntime(authority, certificate.clone());
    match result {
        Ok(current) => {
            if let Some(storage) = crate::storage_helpers::open_storage() {
                storage
                    .set_app_setting(
                        enabledSettingKey,
                        "true",
                        codexmanager_core::storage::now_ts(),
                    )
                    .map_err(|_| "保存直连观测开关失败")?;
            }
            *guard = Some(current);
            Ok(statusOf(guard.as_ref()))
        }
        Err(error) => {
            std::fs::remove_file(&certificate).map_err(|_| "观测启动失败且公开证书清理失败")?;
            Err(error)
        }
    }
}

// 专用 Tokio 线程避免嵌套运行时；ready 只在实际绑定成功后返回地址，线程退出释放所有网络资源。
fn startRuntime(
    authority: certificateAuthority::Authority,
    certificate: PathBuf,
) -> Result<Running, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "初始化观测运行时失败")?;
    let cancel = CancellationToken::new();
    let workerCancel = cancel.clone();
    let proxy = crate::gateway::current_upstream_proxy_url();
    let (sink, databaseThread) =
        recordSink::RecordSink::start(crate::process_env::ensure_default_db_path())?;
    let counters = sink.counters.clone();
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("directObservation".into())
        .spawn(move || {
            runtime.block_on(async move {
                let engine =
                    match transport::Engine::new(authority, sink, proxy, workerCancel).await {
                        Ok(engine) => engine,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                let listener =
                    match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await {
                        Ok(listener) => listener,
                        Err(_) => {
                            let _ = ready.send(Err("绑定观测端口失败".into()));
                            return;
                        }
                    };
                let address = match listener.local_addr() {
                    Ok(address) => format!("http://{address}"),
                    Err(_) => {
                        let _ = ready.send(Err("读取观测端口失败".into()));
                        return;
                    }
                };
                if ready.send(Ok(address)).is_err() {
                    return;
                }
                transport::serve(listener, Arc::new(engine)).await;
            });
            // 网络任务先析构关闭发送端，数据库线程再排空队列，避免停止时漏掉已经完成的费用快照。
            runtime.shutdown_timeout(std::time::Duration::from_secs(5));
            if databaseThread.join().is_err() {
                log::error!("观测数据库线程异常退出");
            }
        })
        .map_err(|_| "启动观测线程失败")?;
    let address = started.recv().map_err(|_| "观测线程提前退出")??;
    Ok(Running {
        address,
        certificate,
        cancel,
        thread,
        counters,
    })
}

// 停止先关闭监听及活动连接，再排空写库队列并清理公开证书；重复停止幂等。
pub fn stop() -> Result<ObservationStatus, String> {
    let mut guard = runningEngine
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "观测状态锁损坏")?;
    if let Some(current) = guard.take() {
        current.cancel.cancel();
        let joined = current.thread.join();
        std::fs::remove_file(&current.certificate).map_err(|_| "清理观测公开证书失败")?;
        joined.map_err(|_| "观测线程异常退出")?;
    }
    if let Some(storage) = crate::storage_helpers::open_storage() {
        storage
            .set_app_setting(
                enabledSettingKey,
                "false",
                codexmanager_core::storage::now_ts(),
            )
            .map_err(|_| "保存直连观测开关失败")?;
    }
    Ok(statusOf(None))
}

// 服务初始化读取持久化开关；默认关闭，旧版本不会意外改变网络路径。
pub fn restoreIfEnabled() {
    let enabled = crate::storage_helpers::open_storage()
        .and_then(|storage| storage.get_app_setting(enabledSettingKey).ok().flatten())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    if enabled {
        if let Err(error) = start() {
            log::error!("恢复直连观测失败：{error}");
        }
    }
}

// 仅作用于用户新启动的终端子进程；保留登录与系统环境，停止后新子进程不再接入。
// 显式清空 NO_PROXY 是为了防止既有通配绕过观测；其它主机由隧道原样转发，不解密。
pub fn configureChild(command: &mut std::process::Command) -> Result<(), String> {
    let state = status()?;
    if !state.running {
        return Ok(());
    }
    let address = state.proxyUrl.ok_or("观测地址缺失")?;
    let certificate = state.certificatePath.ok_or("观测证书缺失")?;
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env(name, &address);
    }
    for name in ["NO_PROXY", "no_proxy"] {
        command.env(name, "localhost,127.0.0.1,::1");
    }
    command.env("CODEX_CA_CERTIFICATE", &certificate);
    Ok(())
}
