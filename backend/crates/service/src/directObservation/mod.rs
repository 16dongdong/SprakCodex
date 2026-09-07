//! 直连观测宿主：线程、TLS 密钥与数据库消费者均在当前进程，不依赖独立程序。
//! 开关默认关闭，用户启用后跨重启保留；服务退出只释放运行资源，不改变用户选择。
#[cfg(windows)]
mod authorityStore;
mod certificateAuthority;
mod clientEvents;
mod clientEventMonitor;
mod loopbackListeners;
#[cfg(windows)]
mod processCatalog;
#[cfg(windows)]
mod nativeInjection;
#[allow(non_snake_case)]
mod processInjector;
mod processMonitor;
mod recordSink;
mod responseIdentity;
mod relayIngress;
mod runtimePaths;
mod streamObserver;
mod transport;
mod usageParser;
mod websocketObservation;
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
    retainCertificate: bool,
    relayConfig: Option<PathBuf>,
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
    // 缺少 DLL 必须在发布监听前报错，避免 UI 的 running 掩盖完全没有接管目标进程的状态。
    let injectionDll = runtimePaths::resolve()?;
    let relayConfig = injectionDll
        .as_deref()
        .map(runtimePaths::configPath)
        .transpose()?;
    crate::storage_helpers::initialize_storage()?;
    let folder = crate::process_env::db_dir();
    let authority = certificateAuthority::Authority::forRuntime(observationHosts, &folder)?;
    let certificate = folder.join("observationAuthority.pem");
    runtimePaths::writeAtomically(&certificate, authority.pem.as_bytes())?;
    let result = startRuntime(authority, certificate.clone(), injectionDll, relayConfig);
    match result {
        Ok(current) => {
            // 运行时已绑定端口后才写入开关；持久化失败必须回收已启动线程和证书，避免出现“界面关闭但端口仍监听”。
            let storage = match crate::storage_helpers::open_storage() {
                Some(storage) => storage,
                None => {
                    if let Err(error) = cleanupRunning(current) {
                        log::error!("直连观测启动回滚失败：{error}");
                    }
                    return Err("打开观测设置失败".into());
                }
            };
            if storage
                .set_app_setting(
                    enabledSettingKey,
                    "true",
                    codexmanager_core::storage::now_ts(),
                )
                .is_err()
            {
                if let Err(error) = cleanupRunning(current) {
                    log::error!("直连观测启动回滚失败：{error}");
                }
                return Err("保存直连观测开关失败".into());
            }
            *guard = Some(current);
            Ok(statusOf(guard.as_ref()))
        }
        Err(error) => {
            // 公开证书属于已发布的信任身份；启动失败也保留它，避免旧客户端重建 TLS 时读到缺失文件。
            Err(error)
        }
    }
}

// 专用 Tokio 线程避免嵌套运行时；ready 只在实际绑定成功后返回地址，线程退出释放所有网络资源。
fn startRuntime(
    authority: certificateAuthority::Authority,
    certificate: PathBuf,
    injectionDll: Option<PathBuf>,
    relayConfig: Option<PathBuf>,
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
    let clientEvents = clientEventMonitor::EventMonitor::start(clientEventMonitor::Settings {
        home: clientEventMonitor::defaultHome()?, since: chrono::Utc::now().timestamp_millis(), allow: Arc::new(|_| true),
    }, sink.clone(), cancel.clone())?;
    let homes = clientEvents.registration();
    let counters = sink.counters.clone();
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let workerConfig = relayConfig.clone();
    let workerCertificate = certificate.clone();
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
                    match loopbackListeners::LoopbackListeners::bind().await {
                        Ok(listener) => listener,
                        Err(_) => {
                            let _ = ready.send(Err("绑定观测端口失败".into()));
                            return;
                        }
                    };
                let port = listener.port();
                let address = format!("http://127.0.0.1:{port}");
                if let Some(configPath) = workerConfig.as_deref() {
                    if let Err(error) = runtimePaths::writeRelayConfig(configPath, port, Some(&workerCertificate)) {
                        let _ = ready.send(Err(error));
                        return;
                    }
                }
                if ready.send(Ok(address)).is_err() {
                    return;
                }
                let monitorEngine = engine.cancel.clone();
                tokio::spawn(async move {
                    let Some(dll) = injectionDll else {
                        log::error!("无法确定观测注入 DLL 路径");
                        return;
                    };
                    processMonitor::run(dll, monitorEngine, processInjector::findCandidates, move |candidate, module| {
                        homes.register(processInjector::runtimeHome(candidate, module)?)
                    }).await;
                });
                transport::serve(listener, Arc::new(engine)).await;
            });
            // 网络任务先析构关闭发送端，数据库线程再排空队列，避免停止时漏掉已经完成的费用快照。
            drop(clientEvents);
            runtime.shutdown_timeout(std::time::Duration::from_secs(5));
            if databaseThread.join().is_err() {
                log::error!("观测数据库线程异常退出");
            }
        })
        .map_err(|_| "启动观测线程失败")?;
    let address = match started
        .recv()
        .map_err(|_| "观测线程提前退出".to_string())
        .and_then(|ready| ready)
    {
        Ok(address) => address,
        Err(error) => {
            cancel.cancel();
            // 接收启动失败后仍 join，让失败实例的数据库线程先退出，避免重试遗留工作线程。
            thread.join().map_err(|_| "观测启动失败且线程异常退出")?;
            if let Some(path) = relayConfig.as_deref() {
                runtimePaths::writeRelayConfig(path, 0, None)?;
            }
            return Err(error);
        }
    };
    Ok(Running {
        address,
        certificate,
        retainCertificate: cfg!(windows),
        relayConfig,
        cancel,
        thread,
        counters,
    })
}

// 用户主动停用会持久化关闭状态；与服务退出区别开，避免正常重启丢失自动恢复配置。
pub fn stop() -> Result<ObservationStatus, String> {
    stopInternal(true)
}

// 服务关闭只释放本次运行的资源，保留用户启用选择；失败供宿主日志记录。
pub fn shutdownRuntime() -> Result<ObservationStatus, String> {
    stopInternal(false)
}

// 统一释放线程、证书和 Relay 配置；启动持久化失败时复用该路径，避免留下半启动状态。
fn cleanupRunning(current: Running) -> Result<(), String> {
    // 先撤销新连接路由再取消 listener；即使文件发布失败也继续回收，运行线程退出会使缓存身份失效。
    let relayResult = current
        .relayConfig
        .as_deref()
        .map(|path| runtimePaths::writeRelayConfig(path, 0, None))
        .transpose();
    current.cancel.cancel();
    let joined = current.thread.join();
    let certificateResult = if !current.retainCertificate && current.certificate.exists() {
        std::fs::remove_file(&current.certificate).map_err(|_| "清理观测公开证书失败".to_string())
    } else {
        Ok(())
    };
    joined.map_err(|_| "观测线程异常退出".to_string())?;
    certificateResult?;
    relayResult.map(|_| ())
}

// 同一锁内串行化开关写入和资源释放；disable 为 false 时绝不写入持久化设置。
fn stopInternal(disable: bool) -> Result<ObservationStatus, String> {
    let mut guard = runningEngine
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "观测状态锁损坏")?;
    let storage = if disable {
        Some(crate::storage_helpers::open_storage().ok_or("打开观测设置失败".to_string()))
    } else {
        None
    };
    let cleanupResult = guard.take().map(cleanupRunning).unwrap_or(Ok(()));
    let persistResult = match storage {
        Some(Ok(storage)) => storage
            .set_app_setting(
                enabledSettingKey,
                "false",
                codexmanager_core::storage::now_ts(),
            )
            .map_err(|_| "保存直连观测开关失败".to_string()),
        Some(Err(error)) => Err(error),
        None => Ok(()),
    };
    cleanupResult?;
    persistResult?;
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

#[cfg(test)]
#[path = "../../tests/observation/liveDirectTests.rs"]
mod liveDirectTests;

#[cfg(all(test, windows))]
#[path = "../../tests/observation/relayRoutingTests.rs"]
mod relayRoutingTests;
