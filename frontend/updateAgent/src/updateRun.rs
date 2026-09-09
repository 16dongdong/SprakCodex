//! 安装事务只处理已验证作业；父进程等待由旧入口或常驻看门狗分别负责。
use std::{fs, path::Path, process::Command, time::Duration};
use update_agent::{validateJob, UpdateJob};

// 兼容旧主程序的一次性调用：验证作业、通知就绪、等待父进程，再执行同一安装事务。
pub(super) fn runLegacy(jobPath: &Path) -> Result<(), String> {
    #[cfg(not(windows))]
    return Err("更新器仅支持 Windows".into());

    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        let job = readJob(jobPath)?;
        validateJob(&job)?;
        let parent = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, job.parentPid) };
        if parent.is_null() {
            return Err("主程序已退出或进程句柄不可访问".into());
        }
        let parent = unsafe { OwnedHandle::from_raw_handle(parent) };
        fs::write(&job.readyPath, b"ready").map_err(|error| error.to_string())?;
        match unsafe { WaitForSingleObject(parent.as_raw_handle(), 120_000) } {
            WAIT_OBJECT_0 => execute(jobPath, &job),
            WAIT_TIMEOUT => Err("等待主程序正常退出超时，未执行安装".into()),
            _ => Err("等待主程序退出失败".into()),
        }
    }
}

// 看门狗已经确认父进程退出并完成映像卸载；这里只执行备份、安装、回滚和重启。
pub(super) fn execute(jobPath: &Path, job: &UpdateJob) -> Result<(), String> {
    let result = executeInner(jobPath, job);
    let message = match &result {
        Ok(()) => "更新安装与启动检查完成".to_string(),
        Err(error) => format!("更新失败：{error}"),
    };
    fs::write(jobPath.with_extension("result.log"), message).map_err(|error| error.to_string())?;
    result
}

// 每次读取都拒绝未知字段；看门狗收到更新命令时和父进程退出后会分别校验一次。
pub(super) fn readJob(jobPath: &Path) -> Result<UpdateJob, String> {
    serde_json::from_slice(&fs::read(jobPath).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

// 安装失败会恢复主程序与更新器；用户数据库、认证和业务配置始终不进入备份目录。
fn executeInner(jobPath: &Path, job: &UpdateJob) -> Result<(), String> {
    #[cfg(not(windows))]
    return Err("更新器仅支持 Windows".into());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        #[allow(non_upper_case_globals)]
        const noWindow: u32 = 0x08000000;
        validateJob(job)?;
        let targetDir = job.targetExe.parent().ok_or("缺少安装目录")?;
        let backup = jobPath.with_extension("rollback");
        fs::create_dir(&backup).map_err(|error| error.to_string())?;
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
                fs::copy(&source, backup.join(name)).map_err(|error| error.to_string())?;
            }
        }
        let install = Command::new(&job.installer)
            .arg("/S")
            .arg(format!("/D={}", targetDir.display()))
            .creation_flags(noWindow)
            .status()
            .map_err(|error| error.to_string());
        let result = match install {
            Ok(status) if status.success() => restartAndCheck(&job.targetExe),
            Ok(status) => Err(format!("安装器退出码：{status}")),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            for name in &names {
                let source = backup.join(name);
                if source.is_file() {
                    fs::copy(source, targetDir.join(name))
                        .map_err(|restore| format!("{error}；恢复失败：{restore}"))?;
                }
            }
            restartAndCheck(&job.targetExe)
                .map_err(|restart| format!("{error}；恢复后启动失败：{restart}"))?;
            return Err(error);
        }
        if job.pendingPath.is_file() {
            fs::remove_file(&job.pendingPath).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

// 安装成功后从固定安装路径启动主程序，并观察十秒存活，部署由新进程与新看门狗握手后恢复。
#[cfg(windows)]
fn restartAndCheck(executable: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new(executable)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_secs(10));
    if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
        return Err(format!("更新后程序提前退出：{status}"));
    }
    Ok(())
}
