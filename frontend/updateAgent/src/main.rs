#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(non_snake_case)]
use std::{fs, path::Path, process::Command, time::Duration};
use update_agent::{validateJob, UpdateJob};

// 更新器按作业路径运行一次；结果只写本机日志，任何失败均以非零码退出。
fn main() {
    let Some(jobPath) = std::env::args_os().nth(1) else {
        std::process::exit(2);
    };
    let jobPath = std::path::PathBuf::from(jobPath);
    let result = runJob(&jobPath);
    let message = match &result {
        Ok(()) => "更新安装与启动检查完成".to_string(),
        Err(e) => format!("更新失败：{e}"),
    };
    if fs::write(jobPath.with_extension("result.log"), message).is_err() {
        std::process::exit(3);
    }
    if result.is_err() {
        std::process::exit(1);
    }
}

// 非 Windows 平台不执行 Windows 安装包。
#[cfg(not(windows))]
fn runJob(_: &Path) -> Result<(), String> {
    Err("更新器仅支持 Windows".into())
}

// Windows 持有父进程句柄避免 PID 重用；就绪前校验，正常退出后才运行安装器，不强杀主程序。
#[cfg(windows)]
fn runJob(jobPath: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    const NO_WINDOW: u32 = 0x08000000;
    let job: UpdateJob = serde_json::from_slice(&fs::read(jobPath).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    validateJob(&job)?;
    let targetDir = job.targetExe.parent().ok_or("缺少安装目录")?;
    let backup = jobPath.with_extension("rollback");
    fs::create_dir(&backup).map_err(|e| e.to_string())?;
    // 只备份当前应用二进制，用户数据库与认证文件不参与复制或回滚。
    let names = [
        job.targetExe
            .file_name()
            .ok_or("程序名称无效")?
            .to_os_string(),
        "observationHook9.dll".into(),
        "updateAgent.exe".into(),
    ];
    for name in &names {
        let source = targetDir.join(name);
        if source.is_file() {
            fs::copy(&source, backup.join(name)).map_err(|e| e.to_string())?;
        }
    }
    let parent = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, job.parentPid) };
    if parent.is_null() {
        return Err("主程序已退出或进程句柄不可访问".into());
    }
    let ready = fs::write(&job.readyPath, b"ready").map_err(|e| e.to_string());
    let waited = if ready.is_ok() {
        unsafe { WaitForSingleObject(parent, 120_000) }
    } else {
        u32::MAX
    };
    unsafe {
        CloseHandle(parent);
    }
    ready?;
    if waited != WAIT_OBJECT_0 {
        return Err("等待主程序正常退出超时，未执行安装".into());
    }
    let result = (|| {
        validateJob(&job)?;
        let install = Command::new(&job.installer)
            .arg("/S")
            .arg(format!("/D={}", targetDir.display()))
            .creation_flags(NO_WINDOW)
            .status()
            .map_err(|e| e.to_string());
        match install {
            Ok(status) if status.success() => restartAndCheck(&job.targetExe),
            Ok(status) => Err(format!("安装器退出码：{status}")),
            Err(error) => Err(error),
        }
    })();
    if let Err(error) = result {
        for name in &names {
            let source = backup.join(name);
            if source.is_file() {
                fs::copy(source, targetDir.join(name))
                    .map_err(|e| format!("{error}；恢复失败：{e}"))?;
            }
        }
        restartAndCheck(&job.targetExe).map_err(|e| format!("{error}；恢复后启动失败：{e}"))?;
        return Err(error);
    }
    if job.pendingPath.is_file() {
        fs::remove_file(&job.pendingPath).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// 安装成功后重新启动原安装路径，观察十秒存活；不强制终止任何新启动的用户进程。
#[cfg(windows)]
fn restartAndCheck(executable: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new(executable)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs(10));
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        return Err(format!("更新后程序提前退出：{status}"));
    }
    Ok(())
}
