use super::{model::UpdateActionResponse, runtime, state};
use std::os::windows::process::CommandExt;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use update_agent::{validateJob, UpdateJob};

// 仅从固定仓库的 HTTPS Release 元数据取得 SHA-256；摘要缺失时拒绝运行安装包，不信任本地作业自报版本。
fn releaseDigest(version: &str, assetName: &str) -> Result<String, String> {
    let parsed = semver::Version::parse(version).map_err(|e| e.to_string())?;
    let url = format!("https://api.github.com/repos/16dongdong/SprakCodex/releases/tags/v{parsed}");
    let release: serde_json::Value = runtime::http_client()?
        .get(url)
        .header("User-Agent", "SprakCodex-Updater")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    if release["draft"].as_bool() != Some(false) {
        return Err("发布状态无效".into());
    }
    release["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a["name"].as_str() == Some(assetName))
        })
        .and_then(|asset| asset["digest"].as_str())
        .and_then(|value| value.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64)
        .map(str::to_owned)
        .ok_or_else(|| "发布附件缺少 SHA-256，未执行更新".into())
}

// 后台命令启动独立更新器；有活动请求或未保存设置时返回等待状态，不强制停止工作。
pub(super) fn apply(app: tauri::AppHandle) -> Result<UpdateActionResponse, String> {
    if crate::app_shell::has_unsaved_settings_draft_sections() {
        return Ok(UpdateActionResponse {
            ok: false,
            message: "等待未保存设置处理完成".into(),
        });
    }
    let pending = state::read_pending_update(&app)?.ok_or("尚未下载更新")?;
    if pending.mode != "installer" {
        return Err("独立更新器需要 Windows NSIS 安装包".into());
    }
    let installer = pending.installer_path.as_ref().ok_or("安装包路径缺失")?;
    let digest = releaseDigest(&pending.latest_version, &pending.asset_name)?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let source = executable
        .parent()
        .ok_or("安装目录缺失")?
        .join("updateAgent.exe");
    if !source.is_file() {
        return Err("安装目录缺少 updateAgent.exe".into());
    }
    let work = state::updates_root_dir(&app)?.join(format!(
        "apply-{}-{}",
        std::process::id(),
        runtime::now_unix_secs()
    ));
    let job = UpdateJob {
        parentPid: std::process::id(),
        installer: installer.into(),
        targetExe: executable,
        expectedSha256: digest,
        currentVersion: env!("CARGO_PKG_VERSION").into(),
        targetVersion: pending.latest_version,
        readyPath: work.join("ready"),
        pendingPath: state::pending_update_path(&app)?,
    };
    validateJob(&job)?;
    if !codexmanager_service::updateActivity::begin_update_drain()? {
        return Ok(UpdateActionResponse {
            ok: false,
            message: "等待进行中的请求完成".into(),
        });
    }
    let staged = work.join("updateAgent.exe");
    let jobPath = work.join("job.json");
    let started = (|| {
        fs::create_dir(&work).map_err(|e| e.to_string())?;
        fs::copy(source, &staged).map_err(|e| e.to_string())?;
        fs::write(
            &jobPath,
            serde_json::to_vec(&job).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut worker = Command::new(staged)
            .arg(&jobPath)
            .creation_flags(runtime::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while !job.readyPath.is_file() {
            if worker.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("更新器启动验证失败，请查看更新日志".into());
            }
            if Instant::now() >= deadline {
                // 父进程仍在运行，工作进程尚未获准安装；结束本次子进程，避免稍后意外应用过期作业。
                worker.kill().map_err(|e| e.to_string())?;
                worker.wait().map_err(|e| e.to_string())?;
                return Err("更新器就绪等待超时".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = started {
        codexmanager_service::updateActivity::cancel_update_drain();
        return Err(error);
    }
    crate::app_shell::prepare_for_forced_app_exit();
    std::thread::spawn(move || {
        // 使用服务自身退出路径关闭监听并提交日志，不由更新器强杀主进程。
        std::thread::sleep(Duration::from_millis(300));
        app.exit(0);
    });
    Ok(UpdateActionResponse {
        ok: true,
        message: "独立更新器已就绪，即将重启".into(),
    })
}
