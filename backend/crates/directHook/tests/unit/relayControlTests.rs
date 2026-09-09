use super::*;
use cpcommon::{relayMemory::Publisher, runtimeLease::currentIdentity};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

// 固定部署标识对应唯一命名对象；串行锁让并行测试不会互相占用或覆盖配置。
struct Fixture {
    publisher: Publisher,
    identity: &'static str,
}
impl Fixture {
    // 每例创建独立映射，既允许测试并行，也不会与正在运行的桌面宿主争用生产对象。
    fn new() -> Self {
        static sequence: AtomicU64 = AtomicU64::new(0);
        let identity = Box::leak(
            format!(
                "relay-control-test-{}-{}",
                std::process::id(),
                sequence.fetch_add(1, Ordering::Relaxed)
            )
            .into_boxed_str(),
        );
        let publisher = Publisher::create(identity).unwrap();
        Self {
            publisher,
            identity,
        }
    }

    // 活跃配置绑定当前测试线程；证书和完成目录留空，不触碰真实网络。
    fn settings() -> RelayConfig {
        RelayConfig {
            relayPort: 32123,
            forceProxyTcp: true,
            owner: Some(currentIdentity().unwrap()),
            caCertificatePath: None,
            completionEnabled: false,
            completionDirectory: None,
        }
    }

    // 序列化后一次发布完整快照；格式错误会直接使测试失败。
    fn publish(&self, settings: &RelayConfig) {
        self.publisher
            .write(&serde_json::to_vec(settings).unwrap())
            .unwrap();
    }
}

// 相同字节复用已验证 Arc；新快照必须整体替换端口和完成目录。
#[test]
fn snapshotsAreCachedAndReplacedAtomically() {
    let fixture = Fixture::new();
    let mut control = RelayControl::forIdentity(fixture.identity);
    assert!(control.read().is_none());
    let mut settings = Fixture::settings();
    fixture.publish(&settings);
    let first = control.read().unwrap();
    assert!(Arc::ptr_eq(&first, &control.read().unwrap()));
    settings.relayPort += 1;
    settings.completionEnabled = true;
    settings.completionDirectory = Some(r"C:\fixture\events".into());
    fixture.publish(&settings);
    let updated = control.read().unwrap();
    assert_eq!(updated.relayPort, 32124);
    assert_eq!(updated.completionDirectory, settings.completionDirectory);
    assert_eq!(first.relayPort, 32123);
}

// 缺 owner、零端口或关闭状态均清空旧路由，随后有效快照仍可重新启用。
#[test]
fn invalidSemanticSnapshotNeverRetainsOldRoute() {
    let fixture = Fixture::new();
    let settings = Fixture::settings();
    let mut control = RelayControl::forIdentity(fixture.identity);
    for invalid in [
        RelayConfig {
            owner: None,
            ..settings.clone()
        },
        RelayConfig {
            relayPort: 0,
            ..settings.clone()
        },
        RelayConfig {
            forceProxyTcp: false,
            ..settings.clone()
        },
    ] {
        fixture.publish(&settings);
        assert!(control.read().is_some());
        fixture.publish(&invalid);
        assert!(control.read().is_none());
        assert!(control.current.is_none());
    }
    fixture.publish(&settings);
    assert!(control.read().is_some());
}

// owner 线程退出后，即使映射字节不变，缓存也必须立即失效。
#[test]
fn runtimeExitInvalidatesCachedConfiguration() {
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
    let mut control = RelayControl::forIdentity(fixture.identity);
    assert!(control.read().is_some());
    exitSender.send(()).unwrap();
    worker.join().unwrap();
    assert!(control.read().is_none());
}
