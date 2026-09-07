use super::*;
use base64::Engine;
use codexmanager_core::storage::Account;
use std::ffi::OsString;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server, StatusCode as TinyStatusCode};

fn jwt_with_json(payload_json: &str) -> String {
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
    format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")
}

struct EnvVarRestore {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarRestore {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }

    fn remove(key: &'static str) -> Self {
        let original = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, original }
    }
}

impl Drop for EnvVarRestore {
    fn drop(&mut self) {
        match self.original.as_ref() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn token_with_refresh(account_id: &str, refresh_token: &str) -> Token {
    Token {
        account_id: account_id.to_string(),
        id_token: "id-token".to_string(),
        access_token: "access-token".to_string(),
        refresh_token: refresh_token.to_string(),
        api_key_access_token: None,
        last_refresh: now_ts(),
    }
}

fn insert_account(storage: &Storage, account_id: &str) {
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: account_id.to_string(),
            label: account_id.to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
}

fn start_region_blocked_refresh_server() -> (
    String,
    std::sync::mpsc::Receiver<String>,
    thread::JoinHandle<()>,
) {
    let server = Server::http("127.0.0.1:0").expect("start refresh mock server");
    let url = format!("http://{}/oauth/token", server.server_addr());
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .expect("refresh mock timeout")
            .expect("receive refresh request");
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read refresh request body");
        tx.send(body).expect("send refresh request body");
        let response = Response::from_string("")
            .with_status_code(TinyStatusCode(403))
            .with_header(
                Header::from_bytes(
                    "x-openai-authorization-error",
                    "unsupported_country_region_territory",
                )
                .expect("auth error header"),
            )
            .with_header(Header::from_bytes("cf-ray", "ray-hkg").expect("cf-ray header"));
        request.respond(response).expect("respond refresh request");
    });
    (url, rx, handle)
}

#[test]
fn recover_refresh_race_uses_latest_token_when_refresh_token_changed() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    insert_account(&storage, "acc-race");
    let mut token = token_with_refresh("acc-race", "refresh-old");
    storage.insert_token(&token).expect("insert old token");
    storage
        .insert_token(&token_with_refresh("acc-race", "refresh-new"))
        .expect("insert new token");

    let recovered = recover_refresh_race_from_latest_token(
        &storage,
        &mut token,
        "refresh-old",
        "refresh token failed with status 400 Bad Request: invalid_grant",
    )
    .expect("recover");

    assert!(recovered);
    assert_eq!(token.refresh_token, "refresh-new");
}

#[test]
fn recover_refresh_race_keeps_error_when_refresh_token_unchanged() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    insert_account(&storage, "acc-no-race");
    let mut token = token_with_refresh("acc-no-race", "refresh-old");
    storage.insert_token(&token).expect("insert token");

    let recovered = recover_refresh_race_from_latest_token(
        &storage,
        &mut token,
        "refresh-old",
        "refresh token failed with status 400 Bad Request: invalid_grant",
    )
    .expect("recover");

    assert!(!recovered);
    assert_eq!(token.refresh_token, "refresh-old");
}

#[test]
fn refresh_and_persist_region_blocked_mock_suspends_account() {
    let _guard = crate::test_env_guard();
    let _ = crate::usage_http::usage_http_client();
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    insert_account(&storage, "acc-region-blocked");
    let mut token = token_with_refresh("acc-region-blocked", "refresh-old");
    storage.insert_token(&token).expect("insert token");
    let (url, rx, handle) = start_region_blocked_refresh_server();
    let _restore = EnvVarRestore::set("CODEX_REFRESH_TOKEN_URL_OVERRIDE", &url);

    let err = refresh_and_persist_access_token(
        &storage,
        &mut token,
        "https://auth.openai.com",
        "client-id",
        token_refresh_ahead_secs(),
    )
    .expect_err("region blocked refresh should fail");
    let body = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("receive refresh request body");
    handle.join().expect("join refresh mock server");

    assert!(body.contains("refresh_token=refresh-old"));
    assert!(err.contains("auth_error=unsupported_country_region_territory"));
    assert!(
        crate::account_status::mark_account_unavailable_for_auth_error(
            &storage,
            "acc-region-blocked",
            &err,
        )
    );
    let account = storage
        .find_account_by_id("acc-region-blocked")
        .expect("find account")
        .expect("account exists");
    assert_eq!(account.status, "unavailable");
    let reasons = storage
        .latest_account_status_reasons(&["acc-region-blocked".to_string()])
        .expect("load reasons");
    assert_eq!(
        reasons.get("acc-region-blocked").map(String::as_str),
        Some(crate::account_status::REFRESH_TOKEN_REGION_BLOCKED_REASON)
    );
    let stored = storage
        .find_token_by_account_id("acc-region-blocked")
        .expect("load token")
        .expect("token exists");
    assert_eq!(stored.refresh_token, "refresh-old");
}

#[test]
fn token_refresh_ahead_secs_defaults_to_one_hour() {
    let _guard = crate::test_env_guard();
    let _restore = EnvVarRestore::remove(ENV_TOKEN_REFRESH_AHEAD_SECS);

    assert_eq!(token_refresh_ahead_secs(), DEFAULT_TOKEN_REFRESH_AHEAD_SECS);
}

#[test]
fn token_refresh_ahead_secs_reads_positive_env() {
    let _guard = crate::test_env_guard();
    let _restore = EnvVarRestore::set(ENV_TOKEN_REFRESH_AHEAD_SECS, "1800");

    assert_eq!(token_refresh_ahead_secs(), 1800);
}

#[test]
fn token_refresh_ahead_secs_ignores_invalid_env() {
    let _guard = crate::test_env_guard();
    let _restore = EnvVarRestore::set(ENV_TOKEN_REFRESH_AHEAD_SECS, "0");

    assert_eq!(token_refresh_ahead_secs(), DEFAULT_TOKEN_REFRESH_AHEAD_SECS);
}

#[test]
fn token_refresh_client_id_prefers_access_token_claim() {
    let token = Token {
        account_id: "acc-client-id".to_string(),
        id_token: jwt_with_json(r#"{"sub":"user-1","client_id":"client-from-id"}"#),
        access_token: jwt_with_json(r#"{"sub":"user-1","client_id":"client-from-access"}"#),
        refresh_token: "refresh-token".to_string(),
        api_key_access_token: None,
        last_refresh: now_ts(),
    };

    assert_eq!(
        token_refresh_client_id(&token, "client-from-env"),
        "client-from-access"
    );
}

#[test]
fn token_refresh_client_id_falls_back_to_id_token_then_env() {
    let token = Token {
        account_id: "acc-client-id-fallback".to_string(),
        id_token: jwt_with_json(r#"{"sub":"user-1","client_id":"client-from-id"}"#),
        access_token: jwt_with_json(r#"{"sub":"user-1"}"#),
        refresh_token: "refresh-token".to_string(),
        api_key_access_token: None,
        last_refresh: now_ts(),
    };
    assert_eq!(
        token_refresh_client_id(&token, "client-from-env"),
        "client-from-id"
    );

    let token_without_claim = Token {
        id_token: jwt_with_json(r#"{"sub":"user-1"}"#),
        ..token
    };
    assert_eq!(
        token_refresh_client_id(&token_without_claim, "client-from-env"),
        "client-from-env"
    );
}
