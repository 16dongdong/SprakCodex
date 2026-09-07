use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const CODEX_LATEST_SYNC_INTERVAL_ENV: &str = "CODEXMANAGER_CODEX_LATEST_SYNC_INTERVAL_SECS";
const DEFAULT_CODEX_LATEST_SYNC_INTERVAL_SECS: u64 = 6 * 60 * 60;
const MIN_CODEX_LATEST_SYNC_INTERVAL_SECS: u64 = 60;

static CODEX_LATEST_SYNC_STARTED: OnceLock<()> = OnceLock::new();

pub(crate) fn ensure_codex_latest_version_sync() {
    CODEX_LATEST_SYNC_STARTED.get_or_init(|| {
        if let Err(err) = thread::Builder::new()
            .name("codex-latest-version-sync".to_string())
            .spawn(codex_latest_version_sync_loop)
        {
            log::warn!("codex latest client_version sync thread failed to start: {err}");
        }
    });
}

fn codex_latest_version_sync_loop() {
    loop {
        match super::sync_gateway_user_agent_version_from_codex_latest() {
            Ok(version) => {
                log::info!("codex latest client_version synced: version={}", version);
            }
            Err(err) => {
                log::warn!("codex latest client_version sync failed: {err}");
            }
        }
        thread::sleep(Duration::from_secs(codex_latest_sync_interval_secs()));
    }
}

fn codex_latest_sync_interval_secs() -> u64 {
    std::env::var(CODEX_LATEST_SYNC_INTERVAL_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_CODEX_LATEST_SYNC_INTERVAL_SECS)
        .max(MIN_CODEX_LATEST_SYNC_INTERVAL_SECS)
}

#[cfg(test)]
#[path = "codex_latest_sync_tests.rs"]
mod tests;
