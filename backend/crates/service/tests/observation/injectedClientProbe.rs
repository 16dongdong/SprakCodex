//! 在 CLI 等待 stdin 的可复现边界安装生产模块；不修改该 CLI 的代理、CA、provider 或登录配置。
use super::{
    super::{nativeInjection, runtimePaths},
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
}
impl Drop for InjectedTarget {
    // 只处理本探针创建的 PID 和独占目录，硬退出也会由 Windows 删除进程公开证书文件。
    fn drop(&mut self) {
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

// 显式 DLL 与已绑定 Relay 均属于隔离目录；等待真实 CLI 的 stdin 提示后注入，随后只提交一个无工具请求。
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
    };
    let module = target.moduleDirectory.join("cphook.dll");
    std::fs::copy(source, &module).unwrap();
    let loopbackProxyPorts = std::env::var("OBSERVATION_TEST_ORIGINAL_PROXY_PORTS")
        .unwrap_or_default()
        .split(',')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u16>().expect("代理端口必须为整数"))
        .collect();
    let settings = RelayConfig {
        relayPort,
        forceProxyTcp: true,
        loopbackProxyPorts,
        owner: Some(currentIdentity().unwrap()),
        caCertificatePath: Some(certificate.to_owned()),
    };
    runtimePaths::writeAtomically(
        &target.moduleDirectory.join("hook.json"),
        &serde_json::to_vec(&settings).unwrap(),
    )
    .unwrap();
    target.child = Some(command.spawn().expect("启动独立测试 CLI"));
    let client = target.child.as_mut().unwrap();
    waitForPrompt(client, &directory.join("clientDiagnostics.log"));
    let identity = nativeInjection::candidate(client.id()).unwrap();
    nativeInjection::inject(&identity, &module).unwrap();
    client
        .stdin
        .take()
        .unwrap()
        .write_all("只回复 OBSERVATION_OK，不要调用工具或读取文件。\n".as_bytes())
        .unwrap();
    let success = waitClient(client);
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
