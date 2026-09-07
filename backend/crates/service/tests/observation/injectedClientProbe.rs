//! 使用生产常驻扫描或 stdin 确定性边界接入独立 CLI；不修改它的代理、CA、provider 或登录配置。
use super::{
    super::{nativeInjection, processInjector, processMonitor, runtimePaths},
    waitClient,
};
use cpcommon::{relayContract::RelayConfig, runtimeLease::currentIdentity};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};

// 回收顺序是目标进程、模块文件；保留外层脱敏请求证据，不遗留注入模块或 hook 配置。
struct InjectedTarget {
    child: Option<Child>,
    moduleDirectory: PathBuf,
    monitor: Option<(
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
        if let Some(child) = self.child.as_mut() {
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

// 独占 DLL 与已绑定 Relay 共用隔离目录；monitored 先开启生产扫描再启动 CLI，其余模式等待 stdin 边界再注入。
pub(super) fn run(
    command: &mut Command,
    directory: &Path,
    certificate: &Path,
    relayPort: u16,
) -> bool {
    let source = PathBuf::from(
        std::env::var_os("OBSERVATION_TEST_NETWORK_DLL").expect("指定本次构建的生产 DLL"),
    );
    assert!(source.is_absolute() && source.is_file());
    let moduleDirectory = directory.join("module");
    std::fs::create_dir(&moduleDirectory).unwrap();
    let mut target = InjectedTarget {
        child: None,
        moduleDirectory,
        monitor: None,
    };
    let module = target.moduleDirectory.join("cphook.dll");
    std::fs::copy(source, &module).unwrap();
    let settings = RelayConfig {
        relayPort,
        forceProxyTcp: true,
        owner: Some(currentIdentity().unwrap()),
        caCertificatePath: Some(certificate.to_owned()),
    };
    runtimePaths::writeAtomically(
        &target.moduleDirectory.join("hook.json"),
        &serde_json::to_vec(&settings).unwrap(),
    )
    .unwrap();
    let monitored = std::env::var("OBSERVATION_TEST_CAPTURE_MODE").as_deref() == Ok("monitored");
    // 隔离选择绑定创建时间和可执行文件，避免测试子进程退出后 PID 复用误选用户其他会话。
    let selectedProcess = std::sync::Arc::new(std::sync::Mutex::new(None));
    if monitored {
        let cancel = tokio_util::sync::CancellationToken::new();
        let workerCancel = cancel.clone();
        let selection = selectedProcess.clone();
        let module = module.clone();
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let startup = std::sync::Mutex::new(Some(ready));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(processMonitor::run(module, workerCancel, move || {
                let expected = selection.lock().unwrap().clone();
                let candidates = processInjector::findCandidates()?
                    .into_iter()
                    .filter(|candidate| expected.as_ref() == Some(candidate))
                    .collect();
                // 第一次目录扫描已发生后再让父测试启动 CLI，构造真实的跨扫描周期首请求边界。
                if let Some(ready) = startup.lock().unwrap().take() {
                    ready.send(()).expect("通知首轮目录扫描完成");
                }
                Ok(candidates)
            }));
        });
        target.monitor = Some((cancel, worker));
        started.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    target.child = Some(command.spawn().expect("启动独立测试 CLI"));
    let client = target.child.as_mut().unwrap();
    if monitored {
        *selectedProcess.lock().unwrap() = Some(nativeInjection::candidate(client.id()).unwrap());
    } else {
        waitForPrompt(client, &directory.join("clientDiagnostics.log"));
        let identity = nativeInjection::candidate(client.id()).unwrap();
        nativeInjection::inject(&identity, &module).unwrap();
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
        *selectedProcess.lock().unwrap() = Some(nativeInjection::candidate(client.id()).unwrap());
        success = waitClient(client);
    }
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
    success
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
