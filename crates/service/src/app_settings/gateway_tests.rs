use super::{
    fetch_codex_latest_version_from_url, sync_gateway_user_agent_version_from_codex_latest_url,
};
use crate::APP_SETTING_GATEWAY_USER_AGENT_VERSION_KEY;
use codexmanager_core::storage::Storage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Response, Server};

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var_os(key);
        match value {
            Some(current) => std::env::set_var(key, current),
            None => std::env::remove_var(key),
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.as_ref() {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn unique_temp_db_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("codexmanager-codex-latest-sync-{unique}.db"))
}

fn spawn_latest_registry(version: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let server = Server::http("127.0.0.1:0").expect("start mock registry server");
    let registry_url = format!("http://{}/latest", server.server_addr());
    let join = std::thread::spawn(move || {
        let request = server.recv().expect("receive mock registry request");
        let response = Response::from_string(format!(
            r#"{{"name":"@openai/codex","version":"{version}"}}"#
        ))
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .expect("build content type header"),
        );
        request
            .respond(response)
            .expect("respond mock registry response");
    });
    (registry_url, join)
}

#[test]
fn fetch_codex_latest_version_reads_registry_payload() {
    let _guard = crate::test_env_guard();
    let (registry_url, join) = spawn_latest_registry("0.120.0");

    let result =
        fetch_codex_latest_version_from_url(registry_url.as_str()).expect("fetch latest version");

    join.join().expect("join mock registry server");
    assert_eq!(result.package_name, "@openai/codex");
    assert_eq!(result.version, "0.120.0");
    assert_eq!(result.dist_tag, "latest");
    assert_eq!(result.registry_url, registry_url);
}

#[test]
fn sync_gateway_user_agent_version_from_codex_latest_persists_runtime_version() {
    let _guard = crate::test_env_guard();
    let db_path = unique_temp_db_path();
    let _db_env = EnvGuard::set("CODEXMANAGER_DB_PATH", Some(&db_path.to_string_lossy()));
    crate::initialize_storage_if_needed().expect("init storage");
    let (registry_url, join) = spawn_latest_registry("0.128.0");

    let version = sync_gateway_user_agent_version_from_codex_latest_url(registry_url.as_str())
        .expect("sync latest version");

    join.join().expect("join mock registry server");
    assert_eq!(version, "0.128.0");
    assert_eq!(crate::current_gateway_user_agent_version(), "0.128.0");

    let storage = Storage::open(&db_path).expect("open storage");
    assert_eq!(
        storage
            .get_app_setting(APP_SETTING_GATEWAY_USER_AGENT_VERSION_KEY)
            .expect("read persisted version"),
        Some("0.128.0".to_string())
    );
    let _ = std::fs::remove_file(db_path);
}
