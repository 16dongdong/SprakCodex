#![allow(non_snake_case)]
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use update_agent::{fileDigest, validateJob, UpdateJob};

// 为纯验证创建隔离文件，不运行模拟安装器、不接触真实安装或用户配置。
fn fixture() -> (PathBuf, UpdateJob) {
    let root = std::env::temp_dir().join(format!(
        "update-agent-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let installer = root.join("SprakCodex_0.6.2_x64-setup.exe");
    let executable = root.join("application.exe");
    fs::write(&installer, b"installer fixture").unwrap();
    fs::write(&executable, b"application fixture").unwrap();
    let job = UpdateJob {
        parentPid: std::process::id(),
        expectedSha256: fileDigest(&installer).unwrap(),
        installer,
        targetExe: executable,
        currentVersion: "0.6.1".into(),
        targetVersion: "0.6.2".into(),
        readyPath: root.join("ready"),
        pendingPath: root.join("pending"),
    };
    (root, job)
}

// 完整摘要与递增版本同时成立才接受，篡改文件、降级、名称不符均拒绝。
#[test]
fn jobValidationRejectsTamperingAndDowngrade() {
    let (root, mut job) = fixture();
    assert!(validateJob(&job).is_ok());
    job.currentVersion = "0.6.2".into();
    assert!(validateJob(&job).is_err());
    job.currentVersion = "0.6.1".into();
    job.targetVersion = "0.6.3".into();
    assert!(validateJob(&job).is_err());
    job.targetVersion = "0.6.2".into();
    fs::write(&job.installer, b"modified").unwrap();
    assert!(validateJob(&job).is_err());
    fs::remove_dir_all(root).unwrap();
}

// 固定向量验证摘要算法，防止大小写与编码处理造成校验失效。
#[test]
fn hashMatchesKnownVector() {
    let (root, job) = fixture();
    fs::write(&job.installer, b"abc").unwrap();
    assert_eq!(
        fileDigest(&job.installer).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    fs::remove_dir_all(root).unwrap();
}

// 更新器必须在父进程仍存活时只进入就绪等待；测试主动终止自建工作进程，验证未提前执行安装。
#[cfg(windows)]
#[test]
fn workerWaitsForParentBeforeInstalling() {
    use std::os::windows::process::CommandExt;
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let (root, job) = fixture();
    let jobPath = root.join("job.json");
    fs::write(&jobPath, serde_json::to_vec(&job).unwrap()).unwrap();
    let mut worker = Command::new(env!("CARGO_BIN_EXE_updateAgent"))
        .arg(&jobPath)
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !job.readyPath.exists() && Instant::now() < deadline {
        if worker.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let ready = job.readyPath.exists();
    let unchanged = fs::read(&job.targetExe).unwrap() == b"application fixture";
    if worker.try_wait().unwrap().is_none() {
        worker.kill().unwrap();
    }
    worker.wait().unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(ready, "更新器没有进入就绪状态");
    assert!(unchanged, "主程序退出前安装目录已改变");
}
