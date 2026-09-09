//! 桌面主进程与独立看门狗的单向生命周期通道；stdin 关闭就是父进程退出的最终信号。
use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Mutex, OnceLock},
    time::Duration,
};
use tauri::{path::BaseDirectory, Manager};
use update_agent::WatchdogCommand;

#[cfg(windows)]
struct WatchdogClient {
    process: Child,
    input: BufWriter<ChildStdin>,
}

#[cfg(windows)]
static watchdogClient: OnceLock<Mutex<Option<WatchdogClient>>> = OnceLock::new();

// Windows 启动时复制资源到按父 PID 隔离的临时文件，避免常驻进程锁住安装目录中的更新器。
#[cfg(windows)]
pub(crate) fn start(app: &tauri::AppHandle) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const createNoWindow: u32 = 0x08000000;
    const createBreakawayFromJob: u32 = 0x01000000;
    let source = app
        .path()
        .resolve("updateAgent.exe", BaseDirectory::Resource)
        .map_err(|error| error.to_string())?;
    if !source.is_file() {
        return Err("安装资源缺少 updateAgent.exe".into());
    }
    let root = std::env::temp_dir().join("desktop-update-watchdogs");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    cleanupStaleCopies(&root)?;
    let parentPid = std::process::id();
    let startedAt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "系统时间早于 UNIX 纪元")?
        .as_nanos();
    let staged = root.join(format!("updateAgent-watchdog-{parentPid}-{startedAt}.exe"));
    fs::copy(&source, &staged).map_err(|error| format!("准备看门狗副本失败：{error}"))?;
    let logPath = app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?
        .join(format!("update-watchdog-{parentPid}.log"));
    let mut process = Command::new(&staged)
        .arg("watchdog")
        .arg(parentPid.to_string())
        .arg(&logPath)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(createNoWindow | createBreakawayFromJob)
        .spawn()
        .map_err(|error| format!("启动更新看门狗失败：{error}"))?;
    let input = process.stdin.take().ok_or("看门狗输入管道缺失")?;
    let output = process.stdout.take().ok_or("看门狗握手管道缺失")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(output)
            .read_line(&mut line)
            .map(|_| line.trim() == "watchdog-ready")
            .unwrap_or(false);
        let _ = sender.send(result);
    });
    if receiver.recv_timeout(Duration::from_secs(10)) != Ok(true) {
        let _ = process.kill();
        let _ = process.wait();
        return Err("更新看门狗握手失败".into());
    }
    *watchdogClient
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "看门狗状态锁损坏")? = Some(WatchdogClient {
        process,
        input: BufWriter::new(input),
    });
    Ok(())
}

// 旧父进程退出后副本不再被锁定；清理失败只记录诊断，当前进程使用唯一 PID 文件继续启动。
#[cfg(windows)]
fn cleanupStaleCopies(root: &std::path::Path) -> Result<(), String> {
    let mut pending = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("exe")
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("updateAgent-watchdog-"))
        {
            if let Err(error) = fs::remove_file(&path) {
                log::debug!("旧看门狗副本仍在使用：{}：{error}", path.display());
                pending.push(path);
            }
        }
    }
    if !pending.is_empty() {
        std::thread::Builder::new()
            .name("updateWatchdogCleanup".into())
            .spawn(move || {
                for _ in 0..60 {
                    pending.retain(|path| match fs::remove_file(path) {
                        Ok(()) => false,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                        Err(_) => true,
                    });
                    if pending.is_empty() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
                for path in pending {
                    log::warn!("旧看门狗副本清理超时：{}", path.display());
                }
            })
            .map_err(|error| format!("启动看门狗副本清理线程失败：{error}"))?;
    }
    Ok(())
}

// 非 Windows 没有跨进程内存载荷，看门狗生命周期为空操作。
#[cfg(not(windows))]
pub(crate) fn start(_: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

// 部署变化同步写入同一父进程管道；写入失败使后续更新明确失败，不静默丢失卸载记录。
#[cfg(windows)]
pub(crate) fn sendDeployment(
    tracked: bool,
    record: cpcommon::deploymentLifecycle::DeploymentRecord,
) -> Result<(), String> {
    let command = if tracked {
        WatchdogCommand::Track { record }
    } else {
        WatchdogCommand::Untrack {
            processId: record.processId,
            createdAt: record.createdAt,
        }
    };
    send(&command)
}

// 非 Windows 不产生部署记录；保留相同回调形状供服务层注册。
#[cfg(not(windows))]
pub(crate) fn sendDeployment(
    _: bool,
    _: cpcommon::deploymentLifecycle::DeploymentRecord,
) -> Result<(), String> {
    Ok(())
}

// 更新作业交给已经持有全部部署记录的同一看门狗，ready 文件仍沿用现有 UI 握手协议。
#[cfg(windows)]
pub(crate) fn prepareUpdate(jobPath: &std::path::Path) -> Result<(), String> {
    send(&WatchdogCommand::Update {
        jobPath: jobPath.to_owned(),
    })
}

// 主程序仍存活时可撤销尚未执行的更新，避免 ready 超时后在未来退出时应用陈旧作业。
#[cfg(windows)]
pub(crate) fn cancelUpdate() -> Result<(), String> {
    send(&WatchdogCommand::CancelUpdate)
}

// 更新调用方可在等待 ready 时检查看门狗状态，提前返回真实退出码。
#[cfg(windows)]
pub(crate) fn ensureRunning() -> Result<(), String> {
    let mut guard = watchdogClient
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "看门狗状态锁损坏")?;
    let client = guard.as_mut().ok_or("更新看门狗尚未启动")?;
    match client
        .process
        .try_wait()
        .map_err(|error| error.to_string())?
    {
        Some(status) => Err(format!("更新看门狗已退出：{status}")),
        None => Ok(()),
    }
}

// 序列化和换行在锁内一次提交，防止并发部署事件交叉成无效 JSON。
#[cfg(windows)]
fn send(command: &WatchdogCommand) -> Result<(), String> {
    let mut guard = watchdogClient
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "看门狗状态锁损坏")?;
    let client = guard.as_mut().ok_or("更新看门狗尚未启动")?;
    if let Some(status) = client
        .process
        .try_wait()
        .map_err(|error| error.to_string())?
    {
        return Err(format!("更新看门狗已退出：{status}"));
    }
    serde_json::to_writer(&mut client.input, command).map_err(|error| error.to_string())?;
    client
        .input
        .write_all(b"\n")
        .and_then(|_| client.input.flush())
        .map_err(|error| format!("写入看门狗命令失败：{error}"))
}
