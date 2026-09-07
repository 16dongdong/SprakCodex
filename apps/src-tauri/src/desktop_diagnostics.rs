use rfd::{MessageButtons, MessageDialog, MessageLevel};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::Manager;

const SETTINGS_FILE_NAME: &str = "diagnostics.json";
const STARTUP_ERROR_FILE_NAME: &str = "startup-error.log";
const STARTUP_ERROR_PREVIEW_LIMIT: usize = 16_000;

static DEBUG_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static DEBUG_MODE_FORCED: AtomicBool = AtomicBool::new(false);
static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);
static STARTUP_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);
static STARTUP_ERROR_PATH: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopDiagnosticsSettings {
    pub debug_mode: bool,
    pub file_logging_enabled: bool,
}

impl Default for DesktopDiagnosticsSettings {
    fn default() -> Self {
        Self {
            debug_mode: false,
            file_logging_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDiagnosticsSettingsPatch {
    pub debug_mode: Option<bool>,
    pub file_logging_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDiagnosticsSnapshot {
    pub debug_mode: bool,
    pub effective_debug_mode: bool,
    pub debug_mode_forced: bool,
    pub file_logging_enabled: bool,
    pub log_dir: String,
    pub startup_error: Option<String>,
}

fn diagnostics_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join(SETTINGS_FILE_NAME))
        .map_err(|err| format!("resolve diagnostics settings path failed: {err}"))
}

pub(crate) fn diagnostics_log_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_log_dir()
        .map_err(|err| format!("resolve desktop log directory failed: {err}"))
}

fn startup_error_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    diagnostics_log_dir(app).map(|path| path.join(STARTUP_ERROR_FILE_NAME))
}

fn load_settings_from_path(path: &Path) -> DesktopDiagnosticsSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn write_settings_to_path(
    path: &Path,
    settings: DesktopDiagnosticsSettings,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create diagnostics settings directory {} failed: {err}",
                parent.display()
            )
        })?;
    }
    let contents = serde_json::to_string_pretty(&settings)
        .map_err(|err| format!("serialize diagnostics settings failed: {err}"))?;
    fs::write(path, format!("{contents}\n")).map_err(|err| {
        format!(
            "write diagnostics settings {} failed: {err}",
            path.display()
        )
    })
}

fn command_line_debug_requested() -> bool {
    std::env::args_os().any(|arg| arg.to_string_lossy().eq_ignore_ascii_case("--debug"))
}

fn apply_runtime_settings(settings: DesktopDiagnosticsSettings) {
    let debug_forced = DEBUG_MODE_FORCED.load(Ordering::Relaxed);
    let effective_debug = settings.debug_mode || debug_forced;
    DEBUG_MODE_ENABLED.store(effective_debug, Ordering::Relaxed);
    FILE_LOGGING_ENABLED.store(settings.file_logging_enabled || debug_forced, Ordering::Relaxed);
}

pub(crate) fn initialize_runtime(app: &tauri::AppHandle) -> DesktopDiagnosticsSettings {
    if command_line_debug_requested() {
        DEBUG_MODE_FORCED.store(true, Ordering::Relaxed);
    }

    let settings = diagnostics_settings_path(app)
        .map(|path| load_settings_from_path(&path))
        .unwrap_or_default();
    apply_runtime_settings(settings);

    if let Ok(path) = startup_error_path(app) {
        let _ = STARTUP_ERROR_PATH.set(path);
    }
    settings
}

pub(crate) fn force_debug_mode() {
    DEBUG_MODE_FORCED.store(true, Ordering::Relaxed);
    DEBUG_MODE_ENABLED.store(true, Ordering::Relaxed);
    FILE_LOGGING_ENABLED.store(true, Ordering::Relaxed);
}

pub(crate) fn should_write_file_log(level: log::Level) -> bool {
    if !file_logging_enabled() {
        return false;
    }
    DEBUG_MODE_ENABLED.load(Ordering::Relaxed) || level <= log::Level::Info
}

pub(crate) fn file_logging_enabled() -> bool {
    FILE_LOGGING_ENABLED.load(Ordering::Relaxed)
}

fn read_startup_error(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let char_count = contents.chars().count();
    if char_count <= STARTUP_ERROR_PREVIEW_LIMIT {
        return Some(contents);
    }
    let preview = contents
        .chars()
        .skip(char_count - STARTUP_ERROR_PREVIEW_LIMIT)
        .collect::<String>();
    Some(format!("...\n{preview}"))
}

pub(crate) fn diagnostics_snapshot(
    app: &tauri::AppHandle,
) -> Result<DesktopDiagnosticsSnapshot, String> {
    let settings_path = diagnostics_settings_path(app)?;
    let settings = load_settings_from_path(&settings_path);
    let log_dir = diagnostics_log_dir(app)?;
    let startup_error = read_startup_error(&log_dir.join(STARTUP_ERROR_FILE_NAME));
    Ok(DesktopDiagnosticsSnapshot {
        debug_mode: settings.debug_mode,
        effective_debug_mode: DEBUG_MODE_ENABLED.load(Ordering::Relaxed),
        debug_mode_forced: DEBUG_MODE_FORCED.load(Ordering::Relaxed),
        file_logging_enabled: settings.file_logging_enabled,
        log_dir: log_dir.display().to_string(),
        startup_error,
    })
}

pub(crate) fn update_settings(
    app: &tauri::AppHandle,
    patch: DesktopDiagnosticsSettingsPatch,
) -> Result<DesktopDiagnosticsSnapshot, String> {
    let settings_path = diagnostics_settings_path(app)?;
    let mut settings = load_settings_from_path(&settings_path);
    if let Some(debug_mode) = patch.debug_mode {
        settings.debug_mode = debug_mode;
    }
    if let Some(file_logging_enabled) = patch.file_logging_enabled {
        settings.file_logging_enabled = file_logging_enabled;
    }
    write_settings_to_path(&settings_path, settings)?;
    apply_runtime_settings(settings);
    diagnostics_snapshot(app)
}

fn format_startup_error_report(
    app: &tauri::AppHandle,
    stage: &str,
    error: &str,
    database_backup: Option<&Path>,
) -> String {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let backup = database_backup
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "not created".to_string());
    format!(
        "CodexManager startup failure\n\
         timestamp: {timestamp}\n\
         version: {}\n\
         os: {}\n\
         arch: {}\n\
         stage: {stage}\n\
         debugMode: {}\n\
         fileLoggingEnabled: {}\n\
         databaseBackup: {backup}\n\
         error: {error}\n",
        app.package_info().version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        DEBUG_MODE_ENABLED.load(Ordering::Relaxed),
        FILE_LOGGING_ENABLED.load(Ordering::Relaxed),
    )
}

fn write_startup_error_report(path: &Path, report: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create startup error directory {} failed: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(path, report)
        .map_err(|err| format!("write startup error log {} failed: {err}", path.display()))
}

fn show_startup_error_dialog(error: &str, report_path: Option<&Path>) {
    let path_message = report_path
        .map(|path| format!("\n\n诊断日志：{}", path.display()))
        .unwrap_or_default();
    let description = format!(
        "CodexManager 启动失败，您的数据库不会被自动删除。\n\n{}{}",
        error, path_message
    );
    MessageDialog::new()
        .set_title("CodexManager 启动失败")
        .set_description(&description)
        .set_level(MessageLevel::Error)
        .set_buttons(MessageButtons::Ok)
        .show();
}

pub(crate) fn report_startup_failure(
    app: &tauri::AppHandle,
    stage: &str,
    error: &str,
    database_backup: Option<&Path>,
) {
    let report_path = startup_error_path(app).ok();
    let report = format_startup_error_report(app, stage, error, database_backup);
    if let Some(path) = report_path.as_deref() {
        if let Err(write_error) = write_startup_error_report(path, &report) {
            eprintln!("{write_error}");
        }
    }
    STARTUP_FAILURE_REPORTED.store(true, Ordering::Relaxed);
    show_startup_error_dialog(error, report_path.as_deref());
}

pub(crate) fn report_build_failure(error: &str) {
    if STARTUP_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }

    let report_path = STARTUP_ERROR_PATH.get();
    if let Some(path) = report_path {
        let report = format!(
            "CodexManager startup failure\n\
             timestamp: {}\n\
             version: {}\n\
             os: {}\n\
             arch: {}\n\
             stage: tauri-build\n\
             error: {error}\n",
            chrono::Utc::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        if let Err(write_error) = write_startup_error_report(path, &report) {
            eprintln!("{write_error}");
        }
    }
    show_startup_error_dialog(error, report_path.map(PathBuf::as_path));
}

#[cfg(test)]
mod tests {
    use super::{
        load_settings_from_path, write_settings_to_path, DesktopDiagnosticsSettings,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("codexmanager-diagnostics-test-{unique}"))
    }

    #[test]
    fn diagnostics_settings_use_bounded_logging_defaults() {
        assert_eq!(
            DesktopDiagnosticsSettings::default(),
            DesktopDiagnosticsSettings {
                debug_mode: false,
                file_logging_enabled: true,
            }
        );
    }

    #[test]
    fn corrupt_diagnostics_settings_fall_back_to_defaults() {
        let root = unique_temp_dir();
        let path = root.join("diagnostics.json");
        fs::create_dir_all(&root).expect("create test dir");
        fs::write(&path, "{not-json").expect("write corrupt settings");
        assert_eq!(
            load_settings_from_path(&path),
            DesktopDiagnosticsSettings::default()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diagnostics_settings_round_trip() {
        let root = unique_temp_dir();
        let path = root.join("diagnostics.json");
        let expected = DesktopDiagnosticsSettings {
            debug_mode: true,
            file_logging_enabled: false,
        };
        write_settings_to_path(&path, expected).expect("write settings");
        assert_eq!(load_settings_from_path(&path), expected);
        let _ = fs::remove_dir_all(root);
    }
}
