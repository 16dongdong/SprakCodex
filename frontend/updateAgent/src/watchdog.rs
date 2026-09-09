//! 常驻看门狗通过父进程私有 stdin 接收部署记录；管道关闭后先卸载映像，再执行可选更新。
use crate::updateRun;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
};
use update_agent::{UpdateJob, WatchdogCommand};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE},
};

// 参数固定为父 PID 与日志路径；父句柄、管道 EOF 和创建时间记录共同限定生命周期。
pub(super) fn run(arguments: &[std::ffi::OsString]) -> Result<(), String> {
    if arguments.len() != 2 {
        return Err("看门狗参数无效".into());
    }
    let parentPid = arguments[0]
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value != 0)
        .ok_or("看门狗父进程标识无效")?;
    let logPath = PathBuf::from(&arguments[1]);
    if !logPath.is_absolute() {
        return Err("看门狗日志路径必须为绝对路径".into());
    }
    let parent = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parentPid) };
    if parent.is_null() {
        return Err("看门狗无法打开主进程".into());
    }
    let parent = unsafe { OwnedHandle::from_raw_handle(parent) };
    println!("watchdog-ready");
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;

    let mut records = BTreeMap::new();
    let mut update = None;
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let result = serde_json::from_str::<WatchdogCommand>(&line)
                    .map_err(|error| format!("看门狗命令无效：{error}"))
                    .and_then(|command| {
                        applyCommand(parentPid, command, &mut records, &mut update)
                    });
                if let Err(error) = result {
                    // 单条控制消息不得终止父进程守护；错误写入专用日志，后续有效部署记录仍会执行卸载。
                    appendLog(&logPath, &error)?;
                }
            }
            Err(error) => {
                appendLog(&logPath, &format!("读取看门狗命令失败：{error}"))?;
                break;
            }
        }
    }
    if unsafe { WaitForSingleObject(parent.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
        return Err("看门狗等待主进程退出失败".into());
    }
    unloadTracked(&records, &logPath)?;
    if let Some((jobPath, job)) = update {
        updateRun::execute(&jobPath, &job)?;
    }
    appendLog(&logPath, "看门狗生命周期完成")
}

// Track／Untrack 只维护已验证记录；Update 在主进程退出前校验并发布 ready 握手。
fn applyCommand(
    parentPid: u32,
    command: WatchdogCommand,
    records: &mut BTreeMap<(u32, u64), cpcommon::deploymentLifecycle::DeploymentRecord>,
    update: &mut Option<(PathBuf, UpdateJob)>,
) -> Result<(), String> {
    match command {
        WatchdogCommand::Track { record } => {
            record.validate().map_err(str::to_owned)?;
            records.insert((record.processId, record.createdAt), record);
        }
        WatchdogCommand::Untrack {
            processId,
            createdAt,
        } => {
            records.remove(&(processId, createdAt));
        }
        WatchdogCommand::Update { jobPath } => {
            if update.is_some() || !jobPath.is_absolute() {
                return Err("看门狗更新作业状态无效".into());
            }
            let job = updateRun::readJob(&jobPath)?;
            if job.parentPid != parentPid {
                return Err("更新作业父进程不匹配".into());
            }
            update_agent::validateJob(&job)?;
            fs::write(&job.readyPath, b"ready").map_err(|error| error.to_string())?;
            *update = Some((jobPath, job));
        }
        WatchdogCommand::CancelUpdate => {
            if let Some((_, job)) = update.take() {
                match fs::remove_file(job.readyPath) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("撤销更新就绪文件失败：{error}")),
                }
            }
        }
    }
    Ok(())
}

// 所有记录逐项卸载并完整记录结果；任一映像拒绝排空时停止更新，避免安装后叠加旧回调。
fn unloadTracked(
    records: &BTreeMap<(u32, u64), cpcommon::deploymentLifecycle::DeploymentRecord>,
    logPath: &Path,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for record in records.values() {
        let message = match cpcommon::deploymentLifecycle::unload(record) {
            Ok(true) => format!("已卸载观测映像 pid={}", record.processId),
            Ok(false) => format!("观测目标已退出或映像已释放 pid={}", record.processId),
            Err(error) => {
                failures.push(format!("pid={}：{error}", record.processId));
                continue;
            }
        };
        if let Err(error) = appendLog(logPath, &message) {
            failures.push(format!(
                "pid={}：记录卸载结果失败：{error}",
                record.processId
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        let message = format!("看门狗卸载失败：{}", failures.join("；"));
        appendLog(logPath, &message)?;
        Err(message)
    }
}

// 日志只追加静态生命周期结果和 PID，不写请求、账号或认证内容。
fn appendLog(path: &Path, message: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{message}").map_err(|error| error.to_string())
}
