use super::{codex_latest_sync_interval_secs, CODEX_LATEST_SYNC_INTERVAL_ENV};

struct EnvGuard {
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(value: Option<&str>) -> Self {
        let previous = std::env::var_os(CODEX_LATEST_SYNC_INTERVAL_ENV);
        match value {
            Some(current) => std::env::set_var(CODEX_LATEST_SYNC_INTERVAL_ENV, current),
            None => std::env::remove_var(CODEX_LATEST_SYNC_INTERVAL_ENV),
        }
        Self { previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.as_ref() {
            std::env::set_var(CODEX_LATEST_SYNC_INTERVAL_ENV, value);
        } else {
            std::env::remove_var(CODEX_LATEST_SYNC_INTERVAL_ENV);
        }
    }
}

#[test]
fn codex_latest_sync_interval_enforces_minimum() {
    let _guard = crate::test_env_guard();
    let _env = EnvGuard::set(Some("5"));

    assert_eq!(codex_latest_sync_interval_secs(), 60);
}
