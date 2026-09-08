use super::*;

// 每例独占临时目录，只创建普通文件；析构按已知路径清理，任一清理错误都使测试失败。
struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    // 隔离路径解析与正式安装目录，不加载 DLL，也不启动任何进程。
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("observationPaths{:032x}", rand::random::<u128>()));
        std::fs::create_dir(&directory).expect("创建测试目录");
        Self { directory }
    }

    // 放置仅供路径解析的普通文件，字节内容不是可执行模块。
    fn module(&self, folder: &str) -> PathBuf {
        let directory = self.directory.join(folder);
        std::fs::create_dir_all(&directory).expect("创建资源目录");
        let module = directory.join(moduleFileName);
        std::fs::write(&module, b"path fixture").expect("创建模块路径夹具");
        module
    }
}

impl Drop for Fixture {
    // 文件均由当前测试创建；先删子目录内容，再删根目录，不吞清理错误。
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.directory).expect("枚举夹具") {
            let entry = entry.expect("读取夹具条目");
            if entry.file_type().expect("读取条目类型").is_dir() {
                for file in std::fs::read_dir(entry.path()).expect("枚举资源") {
                    std::fs::remove_file(file.expect("读取资源文件").path()).expect("删除资源文件");
                }
                std::fs::remove_dir(entry.path()).expect("删除资源目录");
            } else {
                std::fs::remove_file(entry.path()).expect("删除测试文件");
            }
        }
        std::fs::remove_dir(&self.directory).expect("删除测试目录");
    }
}

// 覆盖普通安装和 Tauri 平台资源布局；配置总跟随模块而不是宿主目录。
#[test]
fn installedLayoutsKeepConfigurationBesideModule() {
    for folder in ["", "resources", "Resources"] {
        let fixture = Fixture::new();
        let module = fixture.module(folder);
        let resolved = resolveModule(&fixture.directory.join("desktop.exe"), None).unwrap();
        assert_eq!(resolved, std::fs::canonicalize(&module).unwrap());
        assert_eq!(configPath(&resolved).unwrap().parent(), resolved.parent());
    }
}

// 显式 release DLL 覆盖 debug 宿主时，配置应写入 release，而不是旧实现中的 debug。
#[test]
fn explicitModuleSelectsItsOwnDirectory() {
    let fixture = Fixture::new();
    let module = fixture.module("release");
    let resolved =
        resolveModule(&fixture.directory.join("debug/service.exe"), Some(&module)).unwrap();
    let config = configPath(&resolved).unwrap();
    let certificate = fixture.directory.join("authority.pem");
    std::fs::write(&certificate, b"public path fixture").unwrap();
    writeRelayConfig(&config, 32123, Some(&certificate)).unwrap();
    let current: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(current["proxy_relay_port"], 32123);
    assert_eq!(current["force_proxy_tcp"], true);
    assert_eq!(
        current["ca_certificate_path"],
        certificate.to_str().unwrap()
    );
    #[cfg(windows)]
    {
        let settings: cpcommon::relayContract::RelayConfig =
            serde_json::from_value(current.clone()).unwrap();
        let owner = settings.owner.expect("有效端口必须绑定运行线程");
        assert_eq!(owner, cpcommon::runtimeLease::currentIdentity().unwrap());
        assert!(cpcommon::runtimeLease::RuntimeLease::open(owner)
            .unwrap()
            .isActive());
    }
    assert!(!fixture
        .directory
        .join("debug")
        .join(configFileName)
        .exists());
    writeRelayConfig(&config, 0, None).unwrap();
    let stopped: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(stopped["proxy_relay_port"], 0);
    assert_eq!(stopped["force_proxy_tcp"], false);
    assert!(stopped["runtime_owner"].is_null());
    assert_eq!(
        std::fs::read_dir(config.parent().unwrap()).unwrap().count(),
        2
    );
}

// 配置错误是启动失败，不应自动采用另一个 DLL 或把运行状态标成正常。
#[test]
fn invalidExplicitModuleNeverSelectsAnotherFile() {
    let fixture = Fixture::new();
    fixture.module("");
    let executable = fixture.directory.join("desktop.exe");
    assert!(resolveModule(&executable, Some(Path::new("relative.dll"))).is_err());
    assert!(resolveModule(&executable, Some(&fixture.directory)).is_err());
    assert!(resolveModule(&executable, Some(&fixture.directory.join("missing.dll"))).is_err());
}

// 安装资源缺失时明确失败，防止只有监听却从未接管进程的虚假成功。
#[test]
fn missingInstalledModuleIsAnError() {
    let fixture = Fixture::new();
    assert!(resolveModule(&fixture.directory.join("desktop.exe"), None).is_err());
}

// 旧资源文件不能满足新就绪协议，避免升级后仍装入仅有旧网络能力的模块。
#[test]
fn legacyModuleDoesNotSatisfyCurrentInstall() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("cphook.dll"), b"legacy fixture").unwrap();
    assert!(resolveModule(&fixture.directory.join("desktop.exe"), None).is_err());
}

// 重命名被目标目录阻挡时，不留下带随机名称的半成品配置。
#[test]
fn failedPublishRemovesStagingFile() {
    let fixture = Fixture::new();
    let target = fixture.directory.join(configFileName);
    std::fs::create_dir(&target).unwrap();
    assert!(writeRelayConfig(&target, 32123, None).is_err());
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 1);
}
