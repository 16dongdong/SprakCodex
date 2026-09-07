use crate::commands::shared::open_in_file_manager_blocking;
use crate::desktop_diagnostics::{
    self, DesktopDiagnosticsSettingsPatch, DesktopDiagnosticsSnapshot,
};

#[tauri::command]
pub fn app_diagnostics_settings_get(
    app: tauri::AppHandle,
) -> Result<DesktopDiagnosticsSnapshot, String> {
    desktop_diagnostics::diagnostics_snapshot(&app)
}

#[tauri::command]
pub fn app_diagnostics_settings_set(
    app: tauri::AppHandle,
    patch: DesktopDiagnosticsSettingsPatch,
) -> Result<DesktopDiagnosticsSnapshot, String> {
    desktop_diagnostics::update_settings(&app, patch)
}

#[tauri::command]
pub fn app_diagnostics_open_logs_dir(app: tauri::AppHandle) -> Result<(), String> {
    let log_dir = desktop_diagnostics::diagnostics_log_dir(&app)?;
    std::fs::create_dir_all(&log_dir).map_err(|err| {
        format!(
            "create desktop log directory {} failed: {err}",
            log_dir.display()
        )
    })?;
    open_in_file_manager_blocking(&log_dir.display().to_string())
}
