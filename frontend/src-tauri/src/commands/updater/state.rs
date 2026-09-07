use semver::Version;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tauri::Manager;

use super::model::{PendingUpdate, UpdateCheckResponse};

const PENDING_UPDATE_FILE: &str = "pending-update.json";

#[derive(Debug, Default)]
struct UpdaterState {
    last_check: Option<UpdateCheckResponse>,
    last_error: Option<String>,
}

static UPDATER_STATE: OnceLock<Mutex<UpdaterState>> = OnceLock::new();

/// 函数 `updater_state`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn updater_state() -> &'static Mutex<UpdaterState> {
    UPDATER_STATE.get_or_init(|| Mutex::new(UpdaterState::default()))
}

/// 函数 `set_last_check`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn set_last_check(check: UpdateCheckResponse) {
    if let Ok(mut guard) = updater_state().lock() {
        guard.last_check = Some(check);
        guard.last_error = None;
    }
}

/// 函数 `set_last_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn set_last_error(message: String) {
    if let Ok(mut guard) = updater_state().lock() {
        guard.last_error = Some(message);
    }
}

/// 函数 `clear_last_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn clear_last_error() {
    if let Ok(mut guard) = updater_state().lock() {
        guard.last_error = None;
    }
}

/// 函数 `snapshot_last_state`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn snapshot_last_state() -> (Option<UpdateCheckResponse>, Option<String>) {
    if let Ok(guard) = updater_state().lock() {
        (guard.last_check.clone(), guard.last_error.clone())
    } else {
        (None, Some("读取更新器状态锁失败".to_string()))
    }
}

/// 函数 `updates_root_dir`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn updates_root_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut root = app
        .path()
        .app_data_dir()
        .map_err(|_| "未找到应用数据目录".to_string())?;
    root.push("updates");
    fs::create_dir_all(&root).map_err(|err| format!("创建更新目录失败：{err}"))?;
    Ok(root)
}

/// 函数 `pending_update_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn pending_update_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(updates_root_dir(app)?.join(PENDING_UPDATE_FILE))
}

/// 函数 `read_pending_update`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn read_pending_update(app: &tauri::AppHandle) -> Result<Option<PendingUpdate>, String> {
    let path = pending_update_path(app)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|err| format!("读取待安装更新信息失败：{err}"))?;
    let parsed = serde_json::from_slice::<PendingUpdate>(&bytes)
        .map_err(|err| format!("解析待安装更新信息失败：{err}"))?;
    Ok(Some(parsed))
}

/// 函数 `write_pending_update`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn write_pending_update(
    app: &tauri::AppHandle,
    pending: &PendingUpdate,
) -> Result<(), String> {
    let path = pending_update_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建待安装信息目录失败：{err}"))?;
    }
    let bytes = serde_json::to_vec_pretty(pending)
        .map_err(|err| format!("序列化待安装更新信息失败：{err}"))?;
    fs::write(&path, bytes).map_err(|err| format!("写入待安装更新信息失败：{err}"))
}

/// 函数 `clear_pending_update`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn clear_pending_update(app: &tauri::AppHandle) -> Result<(), String> {
    let path = pending_update_path(app)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|err| format!("删除待安装更新信息失败：{err}"))?;
    }
    Ok(())
}

/// 清理已经完成或已废弃的更新版本目录，保留正在等待安装的版本目录和根日志目录。
pub(crate) fn cleanup_completed_update_artifacts(
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let root = updates_root_dir(app)?;
    let pending = read_pending_update(app)?;
    let pending_applied = pending.as_ref().is_some_and(pending_update_is_applied);
    let pending_release_dir = if pending_applied {
        None
    } else {
        pending
            .as_ref()
            .and_then(|value| release_dir_from_pending(&root, value))
    };

    if pending_applied {
        clear_pending_update(app)?;
    }

    cleanup_update_dirs(&root, pending_release_dir.as_deref())
}

fn cleanup_update_dirs(root: &Path, pending_release_dir: Option<&Path>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|err| format!("读取更新目录失败：{err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取更新目录项失败：{err}"))?;
        let path = entry.path();
        let is_logs_dir = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("logs"));
        if !path.is_dir() || is_logs_dir || pending_release_dir == Some(path.as_path()) {
            continue;
        }

        if let Err(err) = fs::remove_dir_all(&path) {
            log::warn!(
                "清理更新版本目录失败，稍后重试：{}：{}",
                path.display(),
                err
            );
        }
    }
    Ok(())
}

fn release_dir_from_pending(root: &Path, pending: &PendingUpdate) -> Option<PathBuf> {
    let release_dir = PathBuf::from(&pending.asset_path).parent()?.to_path_buf();
    (release_dir.parent() == Some(root)).then_some(release_dir)
}

fn pending_update_is_applied(pending: &PendingUpdate) -> bool {
    let current = Version::parse(env!("CARGO_PKG_VERSION")).ok();
    let target = Version::parse(
        pending
            .latest_version
            .trim()
            .trim_start_matches(['v', 'V']),
    )
    .ok();
    match (current, target) {
        (Some(current), Some(target)) => current >= target,
        _ => false,
    }
}

/// 函数 `script_dir_from_pending`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn script_dir_from_pending(
    pending: &PendingUpdate,
    app: &tauri::AppHandle,
) -> Result<PathBuf, String> {
    let asset_path = PathBuf::from(&pending.asset_path);
    if let Some(parent) = asset_path.parent() {
        return Ok(parent.to_path_buf());
    }
    updates_root_dir(app)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::cleanup_update_dirs;

    #[test]
    fn cleanup_update_dirs_preserves_logs_and_pending_release() {
        let root = std::env::temp_dir().join(format!(
            "codexmanager-updater-state-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let logs = root.join("logs");
        let pending = root.join("v0.5.6");
        let stale = root.join("v0.5.5");
        fs::create_dir_all(&logs).expect("create logs");
        fs::create_dir_all(&pending).expect("create pending");
        fs::create_dir_all(&stale).expect("create stale");

        cleanup_update_dirs(&root, Some(&pending)).expect("cleanup stale updates");
        assert!(logs.is_dir());
        assert!(pending.is_dir());
        assert!(!stale.exists());

        cleanup_update_dirs(&root, None).expect("cleanup completed updates");
        assert!(!pending.exists());
        assert!(logs.is_dir());
        fs::remove_dir_all(&root).expect("remove test root");
    }
}
