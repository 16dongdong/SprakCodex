use super::*;
use cpcommon::runtimeLease::currentIdentity;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

// 文件全由本测试创建，按固定路径清理；测试不加载 DLL、不触碰实际安装目录。
struct Fixture(PathBuf);
impl Fixture {
    // 进程 ID 与单调序号防止并发碰撞；create_dir 遇到残留直接报错，不复用别的测试文件。
    fn new() -> Self {
        static sequence: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "relayControl{}-{}",
            std::process::id(),
            sequence.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory.join("hook.json"))
    }

    // 构造只绑定当前测试线程的活跃配置；不启动监听，也不影响系统路由。
    fn settings() -> RelayConfig {
        RelayConfig {
            relayPort: 32123,
            forceProxyTcp: true,
            owner: Some(currentIdentity().unwrap()),
            caCertificatePath: None,
        }
    }

    // 模拟宿主发布完整 JSON；序列化失败必须使测试失败。
    fn publish(&self, settings: &RelayConfig) {
        std::fs::write(&self.0, serde_json::to_vec(settings).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    // 文件缺失是删除用例的预期状态，其他清理错误不能被忽略。
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("清理测试配置失败：{error}"),
        }
        std::fs::remove_dir(self.0.parent().unwrap()).unwrap();
    }
}

// 无配置先保持原连接，随后在相同 reader 发布配置即可启用，避免首次读取永久缓存 None。
#[test]
fn missingConfigurationCanActivateLater() {
    let fixture = Fixture::new();
    let mut control = RelayControl::default();
    assert!(control.read(&fixture.0).is_none());
    fixture.publish(&Fixture::settings());
    let first = control.read(&fixture.0).unwrap();
    let second = control.read(&fixture.0).unwrap();
    assert_eq!(first.relayPort, 32123);
    assert!(Arc::ptr_eq(&first, &second));
}

// 原缓存只按 mtime 判断；保留相同时间仍必须读出更新后的端口和公开证书位置。
#[test]
fn sameTimestampUpdatesWholeSnapshot() {
    let fixture = Fixture::new();
    let mut settings = Fixture::settings();
    fixture.publish(&settings);
    let mut control = RelayControl::default();
    let first = control.read(&fixture.0).unwrap();
    let modified = std::fs::metadata(&fixture.0).unwrap().modified().unwrap();
    settings.relayPort += 1;
    settings.caCertificatePath = Some(fixture.0.with_extension("pem"));
    fixture.publish(&settings);
    std::fs::File::options()
        .write(true)
        .open(&fixture.0)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let updated = control.read(&fixture.0).unwrap();
    assert_eq!(updated.relayPort, 32124);
    assert_eq!(updated.caCertificatePath, settings.caCertificatePath);
    assert_eq!(first.relayPort, 32123);
    assert_eq!(first.caCertificatePath, None);
}

// 删除、格式损坏、缺少 owner、零端口和停用均须清掉旧配置，恢复有效文件后允许重新接入。
#[test]
fn invalidConfigurationNeverRetainsOldRoute() {
    let fixture = Fixture::new();
    let settings = Fixture::settings();
    let mut control = RelayControl::default();
    let mut absentOwner = settings.clone();
    absentOwner.owner = None;
    let mut zeroPort = settings.clone();
    zeroPort.relayPort = 0;
    let mut disabled = settings.clone();
    disabled.forceProxyTcp = false;
    let invalid = [
        b"{".to_vec(),
        serde_json::to_vec(&absentOwner).unwrap(),
        serde_json::to_vec(&zeroPort).unwrap(),
        serde_json::to_vec(&disabled).unwrap(),
        vec![b' '; maxConfigBytes as usize + 1],
    ];
    for encoded in invalid {
        fixture.publish(&settings);
        assert!(control.read(&fixture.0).is_some());
        std::fs::write(&fixture.0, encoded).unwrap();
        assert!(control.read(&fixture.0).is_none());
        assert!(control.current.is_none());
    }
    fixture.publish(&settings);
    assert!(control.read(&fixture.0).is_some());
    std::fs::remove_file(&fixture.0).unwrap();
    assert!(control.read(&fixture.0).is_none());
}

// runtime 线程退出但宿主和配置都存活时，已经缓存的改连决策仍须关闭；新 runtime 随后可重新接入。
#[test]
fn runtimeExitInvalidatesUnchangedConfiguration() {
    let fixture = Fixture::new();
    let (identitySender, identityReceiver) = std::sync::mpsc::channel();
    let (exitSender, exitReceiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        identitySender.send(currentIdentity().unwrap()).unwrap();
        exitReceiver.recv().unwrap();
    });
    let mut settings = Fixture::settings();
    settings.owner = Some(identityReceiver.recv().unwrap());
    fixture.publish(&settings);
    let mut control = RelayControl::default();
    assert!(control.read(&fixture.0).is_some());
    exitSender.send(()).unwrap();
    worker.join().unwrap();
    assert!(control.read(&fixture.0).is_none());
    assert!(control.read(&fixture.0).is_none());
    fixture.publish(&Fixture::settings());
    assert!(control.read(&fixture.0).is_some());
}
