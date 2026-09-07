//! 文件变化只触发增量读取，数据库写入复用网络观测队列；该层补齐启用前已建立连接的完成事件。
use super::{clientEvents::Journal, recordSink::RecordSink};
use notify::{RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

// home 限定 CLI 数据目录，since 限定事件窗口，allow 在读取内容前限定来源范围。
pub(super) struct Settings {
    pub home: PathBuf,
    pub since: i64,
    pub allow: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
}

// 只拥有本次 watcher 和读取线程；停止时取消订阅并等待资源释放，不终止任何客户端。
pub(super) struct EventMonitor {
    stop: CancellationToken,
    thread: Option<std::thread::JoinHandle<()>>,
    homes: HomeRegistration,
}

// 加载调度只提交已验证的公开目录；实际 watch 在读取线程执行，不阻塞网络工作线程。
#[derive(Clone)]
pub(super) struct HomeRegistration(mpsc::SyncSender<PathBuf>);
impl HomeRegistration {
    // 目录必须绝对定位；队列满或已关闭时向加载方返回错误，后续扫描可重试而不是丢失注册。
    pub(super) fn register(&self, home: PathBuf) -> Result<(), String> {
        if !home.is_absolute() {
            return Err("客户端运行目录必须为绝对路径".into());
        }
        self.0
            .try_send(home)
            .map_err(|_| "客户端目录注册队列暂不可用".into())
    }
}
impl EventMonitor {
    // 返回轻量注册入口，生命周期仍由 EventMonitor 的停止令牌和线程持有。
    pub(super) fn registration(&self) -> HomeRegistration {
        self.homes.clone()
    }
    // 目录是明确的本机 CLI home；初始扫描用于处理监听注册期间的写入，来源回调可限定目标范围。
    pub(super) fn start(
        settings: Settings,
        sink: RecordSink,
        stop: CancellationToken,
    ) -> Result<Self, String> {
        let root = settings.home.join("sessions");
        std::fs::create_dir_all(&root).map_err(|_| "准备客户端事件目录失败")?;
        let root = std::fs::canonicalize(root).map_err(|_| "解析客户端事件目录失败")?;
        let (sender, receiver) = mpsc::sync_channel(1024);
        let rescan = Arc::new(AtomicBool::new(false));
        let overflow = rescan.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if sender.try_send(event).is_err() {
                    overflow.store(true, Ordering::Release);
                }
            })
            .map_err(|_| "初始化客户端事件监听失败")?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|_| "注册客户端事件目录失败")?;
        let workerStop = stop.clone();
        let (homeSender, homeReceiver) = mpsc::sync_channel::<PathBuf>(64);
        let thread = std::thread::Builder::new()
            .name("clientObservation".into())
            .spawn(move || {
                let mut watcher = watcher;
                let mut roots = HashSet::from([root.clone()]);
                let mut waitingHomes = HashSet::new();
                let mut failedHomes = HashMap::<PathBuf, std::time::Instant>::new();
                let mut journals = HashMap::<PathBuf, Journal>::new();
                let mut pending = HashSet::new();
                let mut failures = HashMap::<PathBuf, (std::time::Instant, String)>::new();
                collectFiles(&root, &settings, &mut pending);
                while !workerStop.is_cancelled() {
                    waitingHomes.extend(homeReceiver.try_iter());
                    let mut retryHomes = Vec::new();
                    for home in waitingHomes.drain() {
                        if failedHomes
                            .get(&home)
                            .is_some_and(|time| time.elapsed() < Duration::from_secs(1))
                        {
                            retryHomes.push(home);
                            continue;
                        }
                        let addedRoot = home.join("sessions");
                        let registered = std::fs::create_dir_all(&addedRoot)
                            .and_then(|_| std::fs::canonicalize(&addedRoot))
                            .map_err(|_| ())
                            .and_then(|root| {
                                if !roots.contains(&root) {
                                    watcher
                                        .watch(&root, RecursiveMode::Recursive)
                                        .map_err(|_| ())?;
                                }
                                Ok(root)
                            });
                        if let Ok(addedRoot) = registered {
                            if roots.insert(addedRoot.clone()) {
                                collectFiles(&addedRoot, &settings, &mut pending);
                            }
                            failedHomes.remove(&home);
                        } else {
                            if !failedHomes.contains_key(&home) {
                                sink.counters.errors.fetch_add(1, Ordering::Relaxed);
                                log::error!("注册客户端运行目录失败，保留待重试目录");
                            }
                            failedHomes.insert(home.clone(), std::time::Instant::now());
                            retryHomes.push(home);
                        }
                    }
                    waitingHomes.extend(retryHomes);
                    if rescan.swap(false, Ordering::AcqRel) {
                        for root in &roots {
                            collectFiles(root, &settings, &mut pending);
                        }
                    }
                    let mut retry = Vec::new();
                    for path in pending.drain() {
                        if workerStop.is_cancelled() {
                            break;
                        }
                        if !std::fs::symlink_metadata(&path)
                            .is_ok_and(|metadata| metadata.is_file())
                        {
                            journals.remove(&path);
                            failures.remove(&path);
                            continue;
                        }
                        if failures
                            .get(&path)
                            .is_some_and(|(time, _)| time.elapsed() < Duration::from_secs(1))
                        {
                            retry.push(path);
                            continue;
                        }
                        if let Err(error) = journals.entry(path.clone()).or_default().poll(
                            &path,
                            settings.since,
                            &mut |completed| {
                                sink.clientCompleted(
                                    completed.parsed,
                                    completed.timestamp,
                                    completed.pricingAllowed,
                                )
                            },
                        ) {
                            if failures
                                .get(&path)
                                .is_none_or(|(_, previous)| previous != &error)
                            {
                                sink.counters.errors.fetch_add(1, Ordering::Relaxed);
                                log::error!("客户端观测读取失败：{error}");
                            }
                            failures.insert(path.clone(), (std::time::Instant::now(), error));
                            retry.push(path);
                        } else {
                            failures.remove(&path);
                        }
                    }
                    pending.extend(retry);
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(Ok(event)) => {
                            // 后端也会以成功事件的 Rescan 标志报告通知丢失，不能只处理显式错误或通道溢出。
                            if event.need_rescan() {
                                rescan.store(true, Ordering::Release);
                            }
                            for path in event.paths {
                                if accepted(&path, &settings) {
                                    pending.insert(path);
                                }
                            }
                        }
                        Ok(Err(_)) => {
                            log::error!("客户端目录通知丢失，重新核对目录");
                            rescan.store(true, Ordering::Release);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|_| "启动客户端观测线程失败")?;
        Ok(Self {
            stop,
            thread: Some(thread),
            homes: HomeRegistration(homeSender),
        })
    }
}
impl Drop for EventMonitor {
    // 先停止监听再等待读取线程，线程中的 sender 释放后数据库队列可以完整排空。
    fn drop(&mut self) {
        self.stop.cancel();
        if self.thread.take().unwrap().join().is_err() {
            log::error!("客户端观测线程异常退出");
        }
    }
}

// 候选只接受 JSONL；授权范围在读文件前检查，不输出路径或文件内容。
fn accepted(path: &Path, settings: &Settings) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"))
        && path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        && (settings.allow)(path)
}

// 只枚举元数据；正文在文件被选中后才由流式解析器读取，目录通知溢出可重新核对而不丢请求。
fn collectFiles(root: &Path, settings: &Settings, pending: &mut HashSet<PathBuf>) {
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                log::error!("枚举客户端事件目录失败");
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                log::error!("读取客户端目录条目失败");
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                log::error!("读取客户端条目类型失败");
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file() && accepted(&path, settings) {
                // 修改时间只作读取优化；元数据暂不可用时仍尝试解析，由事件自己的时间确定是否提交。
                let recent = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .is_none_or(|time| time.as_millis() >= settings.since as u128);
                if recent {
                    pending.insert(path);
                }
            }
        }
    }
}

// 默认位置遵循 CLI 的 CODEX_HOME 与用户 home 约定；不读取 auth.json 或改写客户端配置。
pub(super) fn defaultHome() -> Result<PathBuf, String> {
    let home = std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })
        .ok_or("缺少客户端 home 路径")?;
    if !home.is_absolute() {
        return Err("客户端 home 必须为绝对路径".into());
    }
    Ok(home)
}
