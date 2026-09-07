#![allow(non_snake_case)]
use codexmanager_core::storage::{now_ts, Storage};
use codexmanager_service::directObservation;
mod support;

// 使用独立进程的隔离数据库验证服务退出与用户停用不同；不启动监听或注入真实进程。
#[test]
fn hostShutdownPreservesEnabledChoice() {
    let _environmentLock = support::test_env_guard();
    let directory = std::env::temp_dir().join(format!(
        "observationLifecycle{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir(&directory).unwrap();
    let database = directory.join("fixture.db");
    let _path = support::EnvGuard::set("CODEXMANAGER_DB_PATH", database.to_str().unwrap());
    let _idle = support::EnvGuard::set("CODEXMANAGER_STORAGE_MAX_IDLE_CONNECTIONS", "0");
    let storage = Storage::open(&database).unwrap();
    storage.init().unwrap();
    let setting = directObservation::enabledSettingKey;
    storage.set_app_setting(setting, "true", now_ts()).unwrap();
    assert!(!directObservation::shutdownRuntime().unwrap().running);
    assert!(!directObservation::shutdownRuntime().unwrap().running);
    assert_eq!(
        storage.get_app_setting(setting).unwrap().as_deref(),
        Some("true")
    );
    assert!(!directObservation::stop().unwrap().running);
    assert_eq!(
        storage.get_app_setting(setting).unwrap().as_deref(),
        Some("false")
    );
    drop(storage);
    // 只删除本测试创建且位于唯一目录内的数据库文件，不递归删除任何外部路径。
    for entry in std::fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        removeFixtureFile(entry.path());
    }
    let _ = std::fs::remove_dir(directory);
}

// Windows SQLite 的 WAL/SHM 句柄在连接释放后可能延迟关闭，短暂重试可避免清理阶段的竞态。
fn removeFixtureFile(path: std::path::PathBuf) {
    for _ in 0..40 {
        match std::fs::remove_file(&path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => {
                // SQLite 连接池可能在测试进程结束前仍持有句柄，交由临时目录清理器回收。
                eprintln!("观测测试文件暂时无法删除 {}：{error}", path.display());
                return;
            }
        }
    }
    eprintln!("观测测试文件删除延迟：{}", path.display());
}
