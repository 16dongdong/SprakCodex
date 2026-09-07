#![allow(non_snake_case, non_upper_case_globals)]
use codexmanager_core::storage::{now_ts, Storage};
use codexmanager_service::directObservation;
mod support;

const fixtureVariable: &str = "OBSERVATION_LIFECYCLE_FIXTURE";

// 数据库适配层的 SQLx 池可能延迟关闭文件，父进程在测试子进程退出后严格清理，而非吞掉 Windows 共享冲突。
#[test]
fn hostShutdownPreservesEnabledChoice() {
    let directory = std::env::temp_dir().join(format!(
        "observationLifecycle{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir(&directory).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lifecycleWorker", "--ignored", "--nocapture"])
        .env(fixtureVariable, &directory)
        .output()
        .expect("启动生命周期测试子进程失败");
    for entry in std::fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::remove_file(entry.path()).expect("删除测试数据库文件失败");
    }
    std::fs::remove_dir(directory).expect("删除测试目录失败");
    assert!(
        result.status.success(),
        "生命周期子进程失败：{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

// 仅由父测试运行；断言关停不改启用设置、主动停用持久化关闭，重复调用幂等且不触及真实进程。
#[test]
#[ignore = "由父测试提供独立目录并负责进程退出后的清理"]
fn lifecycleWorker() {
    let _environmentLock = support::test_env_guard();
    let directory =
        std::path::PathBuf::from(std::env::var_os(fixtureVariable).expect("缺少父测试目录"));
    let database = directory.join("fixture.db");
    let _path = support::EnvGuard::set("CODEXMANAGER_DB_PATH", database.to_str().unwrap());
    let _idle = support::EnvGuard::set("CODEXMANAGER_STORAGE_MAX_IDLE_CONNECTIONS", "0");
    let storage = Storage::open(&database).unwrap();
    storage.init().unwrap();
    let setting = directObservation::enabledSettingKey;
    storage.set_app_setting(setting, "true", now_ts()).unwrap();
    // 模拟宿主已经退出、但客户端仍持有完成入口的状态；没有 Running 也必须能主动撤销共享开关。
    let completionDirectory = directory.join(cpcommon::completionSpool::directoryName);
    std::fs::create_dir(&completionDirectory).unwrap();
    let control = completionDirectory.join(cpcommon::completionSpool::controlName);
    std::fs::write(
        &control,
        serde_json::to_vec(&cpcommon::completionSpool::Settings {
            enabled: true,
            directory: completionDirectory.clone(),
        })
        .unwrap(),
    )
    .unwrap();
    assert!(!directObservation::shutdownRuntime().unwrap().running);
    assert!(!directObservation::shutdownRuntime().unwrap().running);
    assert_eq!(
        storage.get_app_setting(setting).unwrap().as_deref(),
        Some("true")
    );
    let retained: cpcommon::completionSpool::Settings =
        serde_json::from_slice(&std::fs::read(&control).unwrap()).unwrap();
    assert!(retained.enabled);
    assert!(!directObservation::stop().unwrap().running);
    assert!(!directObservation::stop().unwrap().running);
    assert_eq!(
        storage.get_app_setting(setting).unwrap().as_deref(),
        Some("false")
    );
    let disabled: cpcommon::completionSpool::Settings =
        serde_json::from_slice(&std::fs::read(&control).unwrap()).unwrap();
    assert!(!disabled.enabled);
    std::fs::remove_file(control).unwrap();
    std::fs::remove_dir(completionDirectory).unwrap();
}
