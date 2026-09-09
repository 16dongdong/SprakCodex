//! 使用生产常驻扫描或 stdin 确定性边界接入独立 CLI；不修改它的代理、CA、provider 或登录配置。
use super::{
    super::{nativeInjection, processInjector, processMonitor, runtimePaths},
    waitClient,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};

// 回收顺序是停止扫描并排空加载、目标进程、模块文件；保留外层脱敏请求证据，不遗留模块或配置。
pub(super) struct InjectedTarget {
    pub(super) child: Option<Child>,
    concurrentChildren: Vec<Child>,
    pub(super) moduleDirectory: PathBuf,
    relayPublisher: Option<runtimePaths::RelayPublisher>,
    pub(super) monitor: Option<(
        tokio_util::sync::CancellationToken,
        std::thread::JoinHandle<()>,
    )>,
}
impl Drop for InjectedTarget {
    // 只处理本探针创建的 PID 和独占目录，硬退出也会由 Windows 删除进程公开证书文件。
    fn drop(&mut self) {
        if let Some((cancel, monitor)) = self.monitor.take() {
            cancel.cancel();
            monitor.join().unwrap();
        }
        for child in self
            .child
            .iter_mut()
            .chain(self.concurrentChildren.iter_mut())
        {
            if child.try_wait().unwrap().is_none() {
                child.kill().unwrap();
            }
            child.wait().unwrap();
        }
        for entry in std::fs::read_dir(&self.moduleDirectory).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::remove_file(entry.path()).unwrap();
        }
        std::fs::remove_dir(&self.moduleDirectory).unwrap();
    }
}

// 独占 DLL 与已绑定 Relay 共用隔离目录；顺序/并发模式先扫描后启动，injected 模式单独等待 stdin 边界。
pub(super) fn run(
    command: &mut Command,
    directory: &Path,
    certificate: &Path,
    relayPort: u16,
) -> bool {
    let mut target = prepare(directory, certificate, relayPort);
    let image = runtimePaths::moduleImage().expect("Windows 测试必须包含观测载荷");
    let mode = std::env::var("OBSERVATION_TEST_CAPTURE_MODE").unwrap();
    let monitored = matches!(mode.as_str(), "monitored" | "concurrent");
    // 隔离选择绑定创建时间和可执行文件，避免测试子进程退出后 PID 复用误选用户其他会话。
    let selectedProcess = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    if monitored {
        target.monitor = Some(startMonitor(image, selectedProcess.clone(), None));
    }
    if mode == "concurrent" {
        return runConcurrent(command, &mut target, selectedProcess, directory);
    }
    target.child = Some(command.spawn().expect("启动独立测试 CLI"));
    let client = target.child.as_mut().unwrap();
    if monitored {
        *selectedProcess.lock().unwrap() = vec![nativeInjection::candidate(client.id()).unwrap()];
    } else {
        waitForPrompt(client, &directory.join("clientDiagnostics.log"));
        let identity = nativeInjection::candidate(client.id()).unwrap();
        nativeInjection::inject(&identity, image).unwrap();
        client
            .stdin
            .take()
            .unwrap()
            .write_all("只回复 OBSERVATION_OK，不要调用工具或读取文件。\n".as_bytes())
            .unwrap();
    }
    let mut success = waitClient(client);
    if monitored && success {
        // 复用同一个常驻任务，前一进程退出后再启动另一实例；stdio 句柄保留文件位置以追加独立客户端事件。
        target.child = Some(command.spawn().expect("启动后续独立测试 CLI"));
        let client = target.child.as_mut().unwrap();
        *selectedProcess.lock().unwrap() = vec![nativeInjection::candidate(client.id()).unwrap()];
        success = waitClient(client);
    }
    verifyTrustCleanup(directory);
    success
}

// 同时运行两次官方生成，分别保存 stdout 避免并发管道行交错；候选只包含本探针拥有的两个真实实例。
fn runConcurrent(
    command: &mut Command,
    target: &mut InjectedTarget,
    selected: std::sync::Arc<std::sync::Mutex<Vec<processInjector::ProcessCandidate>>>,
    directory: &Path,
) -> bool {
    let mut outputs = Vec::new();
    for ordinal in 0..2 {
        let output = directory.join(format!("concurrentClient{ordinal}.jsonl"));
        command.stdout(std::fs::File::create(&output).unwrap());
        target
            .concurrentChildren
            .push(command.spawn().expect("启动并发官方 CLI"));
        let child = target.concurrentChildren.last().unwrap();
        selected
            .lock()
            .unwrap()
            .push(nativeInjection::candidate(child.id()).unwrap());
        outputs.push(output);
    }
    assert!(
        target
            .concurrentChildren
            .iter_mut()
            .all(|child| child.try_wait().unwrap().is_none()),
        "两个 CLI 必须存在重叠运行窗口"
    );
    // 两个请求已并发提交；等待必须遍历所有子进程，某一失败也不能跳过另一进程的回收。
    let mut success = true;
    for child in &mut target.concurrentChildren {
        success &= waitClient(child);
    }
    let mut combined = std::fs::File::create(directory.join("clientEvents.jsonl")).unwrap();
    for output in outputs {
        std::io::copy(&mut std::fs::File::open(&output).unwrap(), &mut combined).unwrap();
        std::fs::remove_file(output).unwrap();
    }
    verifyTrustCleanup(directory);
    success
}

// 客户端退出后其公开证书由内核释放，不允许探针以成功返回掩盖生命周期残留。
fn verifyTrustCleanup(directory: &Path) {
    let remainingBundles = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.expect("枚举进程公开证书文件"))
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("observationTrust-")
        })
        .count();
    assert_eq!(
        remainingBundles, 0,
        "测试 CLI 退出后公开证书文件应由内核回收"
    );
}

// 只识别静态启动标记，不输出诊断正文；超时或提前退出使探针失败并由 RAII 回收子进程。
fn waitForPrompt(client: &mut Child, diagnostic: &Path) {
    let started = Instant::now();
    loop {
        assert!(client.try_wait().unwrap().is_none(), "CLI 在提交请求前退出");
        let contents = std::fs::read(diagnostic).unwrap();
        if contents
            .windows(b"Reading prompt from stdin".len())
            .any(|part| part == b"Reading prompt from stdin")
        {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "CLI 没有进入 stdin 等待状态"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

// 共用独占模块与配置准备，不启动客户端或扫描器；返回对象负责回收它拥有的资源。
pub(super) fn prepare(directory: &Path, certificate: &Path, relayPort: u16) -> InjectedTarget {
    let moduleDirectory = directory.join("module");
    std::fs::create_dir(&moduleDirectory).unwrap();
    let target = InjectedTarget {
        child: None,
        concurrentChildren: Vec::new(),
        moduleDirectory,
        relayPublisher: runtimePaths::createRelayPublisher().unwrap(),
        monitor: None,
    };
    let completionDirectory =
        if std::env::var("OBSERVATION_TEST_NATIVE_COMPLETIONS").as_deref() == Ok("true") {
            use cpcommon::completionSpool;
            let events = directory.join(completionSpool::directoryName);
            std::fs::create_dir_all(&events).unwrap();
            Some(events)
        } else {
            None
        };
    runtimePaths::writeRelayConfig(
        target.relayPublisher.as_ref().unwrap(),
        relayPort,
        Some(certificate),
        completionDirectory.as_deref(),
    )
    .unwrap();
    target
}

// 候选按完整进程实例限定；首轮目录扫描完成后返回，加载就绪由各场景独立等待。
pub(super) fn startMonitor(
    image: &'static [u8],
    selectedProcess: std::sync::Arc<std::sync::Mutex<Vec<processInjector::ProcessCandidate>>>,
    homes: Option<super::super::clientEventMonitor::HomeRegistration>,
) -> (
    tokio_util::sync::CancellationToken,
    std::thread::JoinHandle<()>,
) {
    let cancel = tokio_util::sync::CancellationToken::new();
    let workerCancel = cancel.clone();
    let selection = selectedProcess.clone();
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let startup = std::sync::Mutex::new(Some(ready));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(processMonitor::run(
            image,
            workerCancel,
            move || {
                let expected = selection.lock().unwrap().clone();
                let candidates = processInjector::findCandidates()?
                    .into_iter()
                    .filter(|candidate| expected.contains(candidate))
                    .collect();
                // 第一次目录扫描已发生后再让父测试启动 CLI，构造真实的跨扫描周期首请求边界。
                if let Some(ready) = startup.lock().unwrap().take() {
                    ready.send(()).expect("通知首轮目录扫描完成");
                }
                Ok(candidates)
            },
            move |candidate| {
                if let Some(homes) = &homes {
                    homes.register(processInjector::runtimeHome(candidate)?)?;
                }
                Ok(())
            },
        ));
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    (cancel, worker)
}
