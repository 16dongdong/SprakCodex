use crate::commands::settings::sync_window_runtime_state_from_settings;

use super::state::TRAY_AVAILABLE;
use super::window::{
    request_initialize_main_window, request_show_main_window,
};

#[cfg(debug_assertions)]
const DEV_SERVER_APP_URL: &str = "http://127.0.0.1:3005/";
#[cfg(debug_assertions)]
const DEV_SERVER_READY_TIMEOUT_MS: u64 = 60_000;
#[cfg(debug_assertions)]
const DEV_SERVER_READY_INTERVAL_MS: u64 = 500;

/// 函数 `sync_startup_window_state`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 无
pub(crate) fn sync_startup_window_state() {
    if let Ok(mut settings) = codexmanager_service::app_settings_get_with_overrides(
        Some(
            codexmanager_service::current_close_to_tray_on_close_setting()
                && TRAY_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed),
        ),
        Some(TRAY_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed)),
    ) {
        sync_window_runtime_state_from_settings(&mut settings);
    }
}

pub(crate) fn schedule_startup_main_window(app: &tauri::AppHandle) {
    let tray_available = TRAY_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed);
    let configured = codexmanager_service::current_show_main_window_on_startup_setting();
    let action = startup_main_window_action(configured, tray_available);
    let app = app.clone();
    std::thread::spawn(move || {
        wait_for_startup_webview_content();
        match action {
            StartupMainWindowAction::InitializeHidden => {
                log::info!("startup main window remains hidden by app setting after frontend is ready");
                if let Err(err) = request_initialize_main_window(&app) {
                    log::warn!("startup main window initialization request failed: {}", err);
                }
            }
            StartupMainWindowAction::Show => {
                if let Err(err) = request_show_main_window(&app) {
                    log::warn!("startup show main window request failed: {}", err);
                }
            }
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupMainWindowAction {
    InitializeHidden,
    Show,
}

fn startup_main_window_action(configured: bool, tray_available: bool) -> StartupMainWindowAction {
    if configured || !tray_available {
        StartupMainWindowAction::Show
    } else {
        StartupMainWindowAction::InitializeHidden
    }
}

#[cfg(debug_assertions)]
fn wait_for_startup_webview_content() {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(DEV_SERVER_READY_TIMEOUT_MS);
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            log::warn!("create dev server readiness client failed: {}", err);
            return;
        }
    };

    while std::time::Instant::now() <= deadline {
        match client.get(DEV_SERVER_APP_URL).send() {
            Ok(response) if response.status().is_success() => {
                log::info!("dev server app page is ready before main window creation");
                return;
            }
            Ok(response) => {
                log::debug!(
                    "dev server startup page is not ready yet: status={}",
                    response.status()
                );
            }
            Err(err) => {
                log::debug!("dev server startup page readiness probe failed: {}", err);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(
            DEV_SERVER_READY_INTERVAL_MS,
        ));
    }

    log::warn!(
        "dev server app page readiness timed out after {}ms; creating main window anyway",
        DEV_SERVER_READY_TIMEOUT_MS
    );
}

#[cfg(not(debug_assertions))]
fn wait_for_startup_webview_content() {}

#[cfg(test)]
mod tests {
    use super::{startup_main_window_action, StartupMainWindowAction};

    #[test]
    fn startup_window_respects_setting_when_tray_is_available() {
        assert_eq!(
            startup_main_window_action(true, true),
            StartupMainWindowAction::Show
        );
        assert_eq!(
            startup_main_window_action(false, true),
            StartupMainWindowAction::InitializeHidden
        );
    }

    #[test]
    fn startup_window_is_shown_when_tray_is_unavailable() {
        assert_eq!(
            startup_main_window_action(false, false),
            StartupMainWindowAction::Show
        );
    }
}
