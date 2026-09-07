#![allow(non_snake_case)]

use super::*;
use base64::Engine as _;
use codexmanager_core::storage::Account;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

#[path = "../support.rs"]
mod support;
use support::EnvGuard;

// 文件数据库仅用于跨线程可见性测试；目录由当前测试独占，存储与工作线程释放后统一清理。
struct DiskFixture {
    directory: std::path::PathBuf,
}

impl DiskFixture {
    // 创建唯一临时工作区，不使用用户配置或现有数据库；失败立即终止测试。
    fn new() -> Self {
        let sequence = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("tokenLifecycle-{}-{sequence}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    // 返回当前测试唯一的数据库路径，供独立线程打开同一存储。
    fn databasePath(&self) -> std::path::PathBuf {
        self.directory.join("fixture.db")
    }
}

impl Drop for DiskFixture {
    // SQLx 连接池异步关闭文件句柄；只等待 Windows 的共享冲突释放，其他清理错误立即报告。
    fn drop(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            match std::fs::remove_dir_all(&self.directory) {
                Ok(()) => return,
                Err(error)
                    if error.raw_os_error() == Some(32) && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    if std::thread::panicking() {
                        eprintln!("测试已失败，临时数据库清理同时失败：{error}");
                        return;
                    }
                    panic!("清理测试数据库失败：{error}");
                }
            }
        }
    }
}

// 构造不连接上游的令牌样本；仅 exp 与 client_id 参与协议测试，签名不是实际凭据。
fn fixtureToken(accountId: &str) -> Token {
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::json!({"sub": "fixture-user", "exp": now_ts() + 36000, "client_id": "TEST_CLIENT"}).to_string(),
    );
    Token {
        account_id: accountId.to_string(),
        id_token: format!("header.{payload}.fixture"),
        access_token: format!("header.{payload}.fixture"),
        refresh_token: "REFRESH_TOKEN_OLD".to_string(),
        api_key_access_token: None,
        last_refresh: now_ts(),
    }
}

// 每项测试使用独立内存库；返回账号、令牌与存储，初始化失败立即终止测试。
fn fixtureStorage(issuer: &str) -> (Storage, Account, Token) {
    let storage = Storage::open_in_memory().expect("打开测试数据库");
    storage.init().expect("初始化测试数据库");
    let account = Account {
        id: "token-lifecycle-fixture".to_string(),
        label: "测试账号".to_string(),
        issuer: issuer.to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: now_ts(),
        updated_at: now_ts(),
    };
    let token = fixtureToken(&account.id);
    storage.insert_account(&account).expect("保存测试账号");
    storage.insert_token(&token).expect("保存测试令牌");
    (storage, account, token)
}

// 未明确说明 RT 被撤销或过期的 401 只是刷新失败，不应让仍有效的账号永久退出轮询。
#[test]
fn unknownRefresh401DoesNotDisableAccount() {
    let (storage, account, _) = fixtureStorage("http://127.0.0.1");
    let error =
        "refresh token failed with status 401 Unauthorized: unclassified upstream rejection";
    assert!(
        !crate::account_status::mark_account_unavailable_for_auth_error(
            &storage,
            &account.id,
            error
        )
    );
    assert!(
        !crate::account_status::mark_account_unavailable_for_refresh_token_error(
            &storage,
            &account.id,
            error
        )
    );
    assert_eq!(
        storage
            .find_account_by_id(&account.id)
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
}

// 模拟请求持有旧快照而后台已完成轮换；API exchange 必须先读取新快照，不能把旧 RT 写回数据库。
#[test]
fn bearerExchangePreservesLatestRefreshToken() {
    let _environmentGuard = crate::test_env_guard();
    let server = Server::http("127.0.0.1:0").expect("启动本地测试端点");
    let issuer = format!("http://{}", server.server_addr());
    let (storage, account, mut staleToken) = fixtureStorage(&issuer);
    let mut latestToken = staleToken.clone();
    latestToken.refresh_token = "REFRESH_TOKEN_NEW".to_string();
    latestToken.access_token = "ACCESS_TOKEN_NEW".to_string();
    storage.insert_token(&latestToken).expect("模拟并发轮换");
    let responder = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("收到兑换请求");
        request
            .respond(
                Response::from_string(r#"{"access_token":"API_TOKEN_FIXTURE"}"#)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap()),
            )
            .unwrap();
    });
    crate::gateway::gateway_resolve_openai_bearer_token(&storage, &account, &mut staleToken)
        .expect("兑换测试令牌");
    responder.join().expect("等待测试端点退出");
    let stored = storage
        .find_token_by_account_id(&account.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.refresh_token, latestToken.refresh_token);
    assert_eq!(stored.access_token, latestToken.access_token);
}

// 持久化采用版本条件更新；缓存写入和旧刷新响应都不得覆盖已轮换的令牌组。
#[test]
fn conditionalTokenWritesRejectStaleSnapshots() {
    let (storage, account, previous) = fixtureStorage("http://127.0.0.1");
    let mut replacement = previous.clone();
    replacement.refresh_token = "REFRESH_TOKEN_NEW".to_string();
    replacement.access_token = "ACCESS_TOKEN_NEW".to_string();
    assert!(storage
        .replaceTokenIfCurrent(&previous, &replacement, (Some(5000), Some(4000)))
        .unwrap());
    assert!(!storage
        .updateApiTokenIfCurrent(&previous, Some("STALE_API_TOKEN"))
        .unwrap());
    assert!(!storage
        .replaceTokenIfCurrent(&previous, &previous, (None, None))
        .unwrap());
    let stored = storage
        .find_token_by_account_id(&account.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.refresh_token, "REFRESH_TOKEN_NEW");
    assert!(stored.api_key_access_token.is_none());
    assert!(storage
        .updateApiTokenIfCurrent(&stored, Some("API_TOKEN_NEW"))
        .unwrap());
    assert!(!storage.updateApiTokenIfCurrent(&stored, None).unwrap());
    assert_eq!(
        storage
            .find_token_by_account_id(&account.id)
            .unwrap()
            .unwrap()
            .api_key_access_token
            .as_deref(),
        Some("API_TOKEN_NEW")
    );
}

// 明确的过期、撤销与授权失效仍保持终止语义；修复未知 401 不应放宽已确认的身份失效。
#[test]
fn confirmedRefreshFailuresRemainPermanent() {
    let _environmentGuard = crate::test_env_guard();
    for code in [
        "refresh_token_expired",
        "refresh_token_reused",
        "refresh_token_invalidated",
        "invalid_grant",
    ] {
        let body = serde_json::json!({"error":{"code":code}}).to_string();
        let reason = crate::usage_http::classify_refresh_token_auth_error_reason(
            reqwest::StatusCode::UNAUTHORIZED,
            &body,
        )
        .unwrap();
        assert!(reason.isPermanent());
    }
    let (storage, account, _) = fixtureStorage("http://127.0.0.1");
    assert!(
        crate::account_status::mark_account_unavailable_for_auth_error(
            &storage,
            &account.id,
            "refresh token failed with status 400 Bad Request: invalid_grant",
        )
    );
}

// 刷新响应若明确报告账号或工作区停用，不能因同为 401 而误走未知拒绝分支。
#[test]
fn explicitAccountDeactivationRemainsTerminal() {
    let _environmentGuard = crate::test_env_guard();
    for code in ["account_deactivated", "workspace_deactivated"] {
        let (storage, account, _) = fixtureStorage("http://127.0.0.1");
        let message = format!(
            "refresh token failed with status 401 Unauthorized: 刷新失败 [oauth_error={code}]"
        );
        assert!(
            crate::account_status::mark_account_unavailable_for_auth_error(
                &storage,
                &account.id,
                &message
            )
        );
        assert_eq!(
            storage
                .find_account_by_id(&account.id)
                .unwrap()
                .unwrap()
                .status,
            "banned"
        );
    }
}

// 验证前置导出不会为有效 AT 强制轮换，已到期或未知到期时间则仍进入刷新流程。
#[test]
fn usableAccessTokenDoesNotRequireRotation() {
    let mut token = fixtureToken("fresh-export");
    assert!(!accessTokenNeedsRefresh(&token, 3600));
    token.access_token = "OPAQUE_ACCESS_TOKEN".to_string();
    assert!(accessTokenNeedsRefresh(&token, 3600));
}

// 持久化版本检查还必须验证账号归属，禁止把一个账号的令牌误写到另一个账号。
#[test]
fn tokenReplacementRejectsDifferentAccount() {
    let (storage, _, previous) = fixtureStorage("http://127.0.0.1");
    let mut replacement = previous.clone();
    replacement.account_id = "different-account".to_string();
    assert!(storage
        .replaceTokenIfCurrent(&previous, &replacement, (None, None))
        .is_err());
}

// 缺少可刷新凭据时不能把已到期 AT 当作可用 bearer 返回，防止修复误判后反而发送确定无效的授权。
#[test]
fn expiredBearerIsNotReturnedAsUsable() {
    let _environmentGuard = crate::test_env_guard();
    let (storage, account, mut token) = fixtureStorage("http://127.0.0.1");
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"sub":"fixture-user","exp":1}"#);
    token.access_token = format!("header.{payload}.fixture");
    token.id_token.clear();
    token.refresh_token.clear();
    storage.insert_token(&token).unwrap();
    assert!(
        crate::gateway::gateway_resolve_openai_bearer_token(&storage, &account, &mut token)
            .is_err()
    );
}

// 在真实 HTTP 样本中让兑换失败后走 RT 刷新；两个 grant 必须分别使用 ID/AT 自己的 client_id。
#[test]
fn gatewayRefreshUsesAccessTokenClientId() {
    let _environmentGuard = crate::test_env_guard();
    let server = Server::http("127.0.0.1:0").unwrap();
    let issuer = format!("http://{}", server.server_addr());
    let _endpointGuard = EnvGuard::set(
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
        &format!("{issuer}/oauth/token"),
    );
    let (storage, account, mut token) = fixtureStorage(&issuer);
    let idPayload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"sub":"fixture-user","client_id":"EXCHANGE_CLIENT"}"#);
    token.id_token = format!("header.{idPayload}.fixture");
    storage.insert_token(&token).unwrap();
    let responder = std::thread::spawn(move || {
        let mut bodies = Vec::new();
        for index in 0..3 {
            let mut request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .expect("收到 grant 请求");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            bodies.push(
                url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect::<HashMap<String, String>>(),
            );
            let (status, payload) = match index {
                0 => (400, r#"{"error":{"code":"invalid_id_token"}}"#),
                1 => (
                    200,
                    r#"{"access_token":"ACCESS_TOKEN_NEW","refresh_token":"REFRESH_TOKEN_NEW"}"#,
                ),
                _ => (200, r#"{"access_token":"API_TOKEN_NEW"}"#),
            };
            request
                .respond(
                    Response::from_string(payload)
                        .with_status_code(status)
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json").unwrap(),
                        ),
                )
                .unwrap();
        }
        bodies
    });
    let bearer =
        crate::gateway::gateway_resolve_openai_bearer_token(&storage, &account, &mut token)
            .unwrap();
    let bodies = responder.join().unwrap();
    assert_eq!(
        bodies[0].get("client_id").map(String::as_str),
        Some("EXCHANGE_CLIENT")
    );
    assert_eq!(
        bodies[1].get("grant_type").map(String::as_str),
        Some("refresh_token")
    );
    assert_eq!(
        bodies[1].get("client_id").map(String::as_str),
        Some("TEST_CLIENT")
    );
    assert_eq!(bearer, "API_TOKEN_NEW");
    assert_eq!(token.refresh_token, "REFRESH_TOKEN_NEW");
}

// 上游收到完整轮换请求后断开连接，客户端不得以重建连接为由再次发送同一个 RT。
#[test]
fn acceptedRefreshRequestIsNotBlindlyReplayed() {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::time::Instant;
    let _environmentGuard = crate::test_env_guard();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/oauth/token", listener.local_addr().unwrap());
    let _endpointGuard = EnvGuard::set("CODEX_REFRESH_TOKEN_URL_OVERRIDE", &endpoint);
    let responder = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut bodyLength = 0;
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                bodyLength = value.trim().parse::<usize>().unwrap();
            }
        }
        assert!(bodyLength > 0);
        reader.read_exact(&mut vec![0; bodyLength]).unwrap();
        drop(reader);
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok(_) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("测试监听失败：{error}"),
            }
        }
        false
    });
    assert!(
        crate::usage_http::refresh_access_token(&endpoint, "TEST_CLIENT", "REFRESH_TOKEN_OLD")
            .is_err()
    );
    assert!(!responder.join().unwrap(), "轮换请求已提交后不应盲目重放");
}

// 让并发调用持有同一旧快照；实际网络只允许一次 RT grant，所有线程都必须读到同一轮换结果。
#[test]
fn concurrentRefreshesConsumeGrantOnce() {
    let _environmentGuard = crate::test_env_guard();
    let disk = DiskFixture::new();
    let databasePath = disk.databasePath();
    let server = Server::http("127.0.0.1:0").unwrap();
    let issuer = format!("http://{}", server.server_addr());
    let _endpointGuard = EnvGuard::set(
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
        &format!("{issuer}/oauth/token"),
    );
    let (_, account, original) = fixtureStorage(&issuer);
    let storage = Storage::open(&databasePath).unwrap();
    storage.init().unwrap();
    storage.insert_account(&account).unwrap();
    storage.insert_token(&original).unwrap();
    let responder = std::thread::spawn(move || {
        let first = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("收到首个刷新请求");
        first
            .respond(
                Response::from_string(
                    r#"{"access_token":"ACCESS_TOKEN_NEW","refresh_token":"REFRESH_TOKEN_NEW"}"#,
                )
                .with_header(Header::from_bytes("Content-Type", "application/json").unwrap()),
            )
            .unwrap();
        let mut repeated = 0;
        while let Some(request) = server.recv_timeout(Duration::from_millis(500)).unwrap() {
            repeated += 1;
            request
                .respond(
                    Response::from_string(r#"{"error":{"code":"refresh_token_reused"}}"#)
                        .with_status_code(401),
                )
                .unwrap();
        }
        repeated
    });
    let barrier = Arc::new(std::sync::Barrier::new(4));
    let workers = (0..4)
        .map(|_| {
            let barrier = barrier.clone();
            let databasePath = databasePath.clone();
            let issuer = issuer.clone();
            let mut token = original.clone();
            std::thread::spawn(move || {
                let storage = Storage::open(databasePath).unwrap();
                barrier.wait();
                refresh_and_persist_access_token(
                    &storage,
                    &mut token,
                    RefreshTokenOptions {
                        issuer: &issuer,
                        clientId: "TEST_CLIENT",
                        aheadSecs: 3600,
                    },
                )
                .unwrap();
                token.refresh_token
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), "REFRESH_TOKEN_NEW");
    }
    assert_eq!(responder.join().unwrap(), 0);
}

// 派生兑换期间从另一连接观察数据库，证明新 RT 已先保存；兑换失败不影响 OAuth 刷新成功。
#[test]
fn rotatedTokenIsSavedBeforeOptionalExchange() {
    let _environmentGuard = crate::test_env_guard();
    let disk = DiskFixture::new();
    let databasePath = disk.databasePath();
    let server = Server::http("127.0.0.1:0").unwrap();
    let issuer = format!("http://{}", server.server_addr());
    let _endpointGuard = EnvGuard::set(
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
        &format!("{issuer}/oauth/token"),
    );
    let (_, account, mut token) = fixtureStorage(&issuer);
    let storage = Storage::open(&databasePath).unwrap();
    storage.init().unwrap();
    storage.insert_account(&account).unwrap();
    storage.insert_token(&token).unwrap();
    let accountId = account.id.clone();
    let responder = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("收到轮换请求");
        request.respond(Response::from_string(r#"{"access_token":"ACCESS_TOKEN_NEW","refresh_token":"REFRESH_TOKEN_NEW","id_token":"ID_TOKEN_NEW"}"#)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())).unwrap();
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("收到派生兑换请求");
        let observer = Storage::open(databasePath).unwrap();
        let visible = observer
            .find_token_by_account_id(&accountId)
            .unwrap()
            .unwrap();
        request
            .respond(Response::from_string("temporary exchange failure").with_status_code(503))
            .unwrap();
        visible.refresh_token
    });
    refresh_and_persist_access_token(
        &storage,
        &mut token,
        RefreshTokenOptions {
            issuer: &issuer,
            clientId: "TEST_CLIENT",
            aheadSecs: 3600,
        },
    )
    .unwrap();
    assert_eq!(responder.join().unwrap(), "REFRESH_TOKEN_NEW");
    assert_eq!(token.refresh_token, "REFRESH_TOKEN_NEW");
}
