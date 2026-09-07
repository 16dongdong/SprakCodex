use super::{
    delete_accounts_by_statuses, delete_banned_accounts, delete_unavailable_free_accounts,
};
use codexmanager_core::storage::{Account, Storage, Token, UsageSnapshotRecord};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::test_env_guard;

static CLEANUP_TEST_DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

/// 函数 `new_test_dir`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - prefix: 参数 prefix
///
/// # 返回
/// 返回函数执行结果
fn new_test_dir(prefix: &str) -> PathBuf {
    let seq = CLEANUP_TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut dir = std::env::temp_dir();
    dir.push(format!("{prefix}-{}-{seq}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

struct EnvGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvGuard {
    /// 函数 `set`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - key: 参数 key
    /// - value: 参数 value
    ///
    /// # 返回
    /// 返回函数执行结果
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    /// 函数 `drop`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 无
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn account(id: &str, status: &str, sort: i64) -> Account {
    Account {
        id: id.to_string(),
        label: id.to_string(),
        issuer: "chatgpt".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort,
        status: status.to_string(),
        created_at: 1,
        updated_at: 1,
    }
}

fn token(account_id: &str) -> Token {
    Token {
        account_id: account_id.to_string(),
        id_token: "id".to_string(),
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        api_key_access_token: None,
        last_refresh: 1,
    }
}

fn free_usage(account_id: &str) -> UsageSnapshotRecord {
    UsageSnapshotRecord {
        account_id: account_id.to_string(),
        used_percent: Some(95.0),
        window_minutes: Some(300),
        resets_at: None,
        secondary_used_percent: None,
        secondary_window_minutes: None,
        secondary_resets_at: None,
        credits_json: Some(r#"{"planType":"free"}"#.to_string()),
        captured_at: 1,
    }
}

#[test]
fn delete_unavailable_free_accounts_scans_total_but_loads_cleanup_status_candidates() {
    let _lock = test_env_guard();
    let dir = new_test_dir("cleanup-unavailable-free-accounts");
    let db_path = dir.join("codexmanager.db");
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
    for account in [
        account("acc-active", "active", 1),
        account("acc-unavailable-free", "unavailable", 2),
        account("acc-disabled-free", "disabled", 3),
    ] {
        storage.insert_account(&account).expect("insert account");
        storage
            .insert_token(&token(&account.id))
            .expect("insert token");
        storage
            .insert_usage_snapshot(&free_usage(&account.id))
            .expect("insert usage");
    }

    let result = delete_unavailable_free_accounts().expect("cleanup result");

    assert_eq!(result.scanned, 3);
    assert_eq!(result.deleted, 1);
    assert_eq!(result.skipped_disabled, 1);
    assert_eq!(
        result.deleted_account_ids,
        vec!["acc-unavailable-free".to_string()]
    );
    let remaining = Storage::open(&db_path)
        .expect("reopen db")
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .map(|account| account.id)
        .collect::<Vec<_>>();
    assert_eq!(
        remaining,
        vec!["acc-active".to_string(), "acc-disabled-free".to_string()]
    );
}

/// 函数 `delete_banned_accounts_removes_only_banned_accounts`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn delete_banned_accounts_removes_only_banned_accounts() {
    let _lock = test_env_guard();
    let dir = new_test_dir("cleanup-banned-accounts");
    let db_path = dir.join("codexmanager.db");
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
    storage
        .insert_account(&Account {
            id: "acc-banned".to_string(),
            label: "Banned".to_string(),
            issuer: "chatgpt".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 1,
            status: "banned".to_string(),
            created_at: 1,
            updated_at: 1,
        })
        .expect("insert banned");
    storage
        .insert_account(&Account {
            id: "acc-active".to_string(),
            label: "Active".to_string(),
            issuer: "chatgpt".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 2,
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
        })
        .expect("insert active");

    let result = delete_banned_accounts().expect("cleanup result");
    assert_eq!(result.deleted, 1);
    assert_eq!(result.deleted_account_ids, vec!["acc-banned".to_string()]);
    assert!(Storage::open(&db_path)
        .expect("reopen db")
        .find_account_by_id("acc-banned")
        .expect("find banned")
        .is_none());
    assert!(Storage::open(&db_path)
        .expect("reopen db")
        .find_account_by_id("acc-active")
        .expect("find active")
        .is_some());
}

#[test]
fn delete_accounts_by_statuses_removes_selected_statuses_only() {
    let _lock = test_env_guard();
    let dir = new_test_dir("cleanup-accounts-by-status");
    let db_path = dir.join("codexmanager.db");
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
    for (idx, (id, status)) in [
        ("acc-active", "active"),
        ("acc-unavailable", "unavailable"),
        ("acc-banned", "banned"),
        ("acc-limited", "limited"),
        ("acc-disabled", "disabled"),
        ("acc-unknown", "unknown"),
    ]
    .into_iter()
    .enumerate()
    {
        storage
            .insert_account(&Account {
                id: id.to_string(),
                label: id.to_string(),
                issuer: "chatgpt".to_string(),
                chatgpt_account_id: None,
                workspace_id: None,
                group_name: None,
                sort: idx as i64,
                status: status.to_string(),
                created_at: 1,
                updated_at: 1,
            })
            .expect("insert account");
    }

    let result = delete_accounts_by_statuses(vec![
        "banned".to_string(),
        "limited".to_string(),
        "unknown".to_string(),
        "banned".to_string(),
    ])
    .expect("cleanup result");

    assert_eq!(result.deleted, 3);
    assert_eq!(
        result.target_statuses,
        vec![
            "banned".to_string(),
            "limited".to_string(),
            "unknown".to_string()
        ]
    );
    assert_eq!(
        result.deleted_account_ids,
        vec![
            "acc-banned".to_string(),
            "acc-limited".to_string(),
            "acc-unknown".to_string()
        ]
    );
    let remaining = Storage::open(&db_path)
        .expect("reopen db")
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .map(|account| account.id)
        .collect::<Vec<_>>();
    assert_eq!(
        remaining,
        vec![
            "acc-active".to_string(),
            "acc-unavailable".to_string(),
            "acc-disabled".to_string()
        ]
    );
}

#[test]
fn delete_accounts_by_statuses_rejects_active_status() {
    let err = delete_accounts_by_statuses(vec!["active".to_string()])
        .expect_err("active should not be cleanup-selectable");

    assert!(err.contains("unsupported cleanup statuses"));
}
