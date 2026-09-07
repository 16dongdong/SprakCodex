use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use codexmanager_core::auth::{extract_client_id_claim, extract_token_exp, DEFAULT_CLIENT_ID};
use codexmanager_core::storage::{now_ts, Account, Storage, Token};

use crate::account_status::mark_account_unavailable_for_auth_error;
use crate::auth_tokens;
use crate::usage_token_refresh::{
    readLatestToken, refresh_and_persist_access_token, token_refresh_ahead_secs,
    RefreshTokenOptions,
};

const ACCOUNT_TOKEN_EXCHANGE_LOCK_TTL_SECS: i64 = 30 * 60;
const ACCOUNT_TOKEN_EXCHANGE_LOCK_CLEANUP_INTERVAL_SECS: i64 = 60;
const API_KEY_ACCESS_TOKEN_REFRESH_AHEAD_SECS: i64 = 60;

struct AccountTokenExchangeLockEntry {
    lock: Arc<Mutex<()>>,
    last_seen_at: i64,
}

#[derive(Default)]
struct AccountTokenExchangeLockTable {
    entries: HashMap<String, AccountTokenExchangeLockEntry>,
    last_cleanup_at: i64,
}

static ACCOUNT_TOKEN_EXCHANGE_LOCKS: OnceLock<Mutex<AccountTokenExchangeLockTable>> =
    OnceLock::new();

/// 函数 `account_token_exchange_lock`
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
pub(super) fn account_token_exchange_lock(account_id: &str) -> Arc<Mutex<()>> {
    let lock = ACCOUNT_TOKEN_EXCHANGE_LOCKS
        .get_or_init(|| Mutex::new(AccountTokenExchangeLockTable::default()));
    let mut table = crate::lock_utils::lock_recover(lock, "account_token_exchange_locks");
    let now = now_ts();
    maybe_cleanup_exchange_locks(&mut table, now);
    let entry = table
        .entries
        .entry(account_id.to_string())
        .or_insert_with(|| AccountTokenExchangeLockEntry {
            lock: Arc::new(Mutex::new(())),
            last_seen_at: now,
        });
    entry.last_seen_at = now;
    entry.lock.clone()
}

/// 函数 `maybe_cleanup_exchange_locks`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - table: 参数 table
/// - now: 参数 now
///
/// # 返回
/// 无
fn maybe_cleanup_exchange_locks(table: &mut AccountTokenExchangeLockTable, now: i64) {
    if table.last_cleanup_at != 0
        && now.saturating_sub(table.last_cleanup_at)
            < ACCOUNT_TOKEN_EXCHANGE_LOCK_CLEANUP_INTERVAL_SECS
    {
        return;
    }
    table.last_cleanup_at = now;
    table.entries.retain(|_, entry| {
        let stale = now.saturating_sub(entry.last_seen_at) > ACCOUNT_TOKEN_EXCHANGE_LOCK_TTL_SECS;
        !stale || Arc::strong_count(&entry.lock) > 1
    });
}

fn usable_api_key_access_token(value: &str) -> Option<String> {
    let token = value.trim();
    if token.is_empty() {
        return None;
    }
    if access_token_expires_within(token, API_KEY_ACCESS_TOKEN_REFRESH_AHEAD_SECS) {
        return None;
    }
    Some(token.to_string())
}

fn access_token_expires_within(token: &str, ahead_secs: i64) -> bool {
    extract_token_exp(token)
        .map(|exp| exp <= now_ts().saturating_add(ahead_secs))
        .unwrap_or(false)
}

// 兑换只更新派生缓存字段，不能用请求快照覆盖已轮换的 OAuth 令牌；数据库错误继续向上返回。
fn exchange_and_persist_api_key_access_token(
    storage: &Storage,
    token: &mut Token,
    issuer: &str,
    client_id: &str,
) -> Result<String, String> {
    let subject_token = api_key_exchange_subject_token(token)
        .ok_or_else(|| "id_token is unavailable for API key token exchange".to_string())?;
    let exchanged = auth_tokens::obtain_api_key(issuer, client_id, &subject_token)?;
    storage
        .updateApiTokenIfCurrent(token, Some(&exchanged))
        .map_err(|error| format!("保存 API 令牌失败：{error}"))?;
    *token = readLatestToken(storage, &token.account_id)?;
    token
        .api_key_access_token
        .as_deref()
        .and_then(usable_api_key_access_token)
        .map(Ok)
        .unwrap_or_else(|| fallback_to_access_token(token, "兑换期间登录态已更新"))
}

fn api_key_exchange_subject_token(token: &Token) -> Option<String> {
    // `/oauth/token` uses the token-exchange grant and expects the OAuth ID
    // token as its subject.  An access token is the bearer fallback for the
    // upstream request; sending it to this endpoint produces misleading
    // "Invalid ID token" / audience errors even when the account is usable.
    let id_token = token.id_token.trim();
    (!id_token.is_empty()).then(|| id_token.to_string())
}

pub(crate) fn api_key_exchange_client_id(token: &Token, fallback_client_id: &str) -> String {
    // The exchange subject is the ID token, so prefer its client_id claim.  A
    // separately issued access token can carry a different audience/client
    // claim and must not override the ID-token exchange client.
    extract_client_id_claim(&token.id_token)
        .or_else(|| extract_client_id_claim(&token.access_token))
        .or_else(|| {
            let fallback = fallback_client_id.trim();
            (!fallback.is_empty()).then(|| fallback.to_string())
        })
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

// API 派生不可用时仅返回仍有效的 AT；空值或确定到期均返回原错误，不把过期凭据继续交给上游。
fn fallback_to_access_token(token: &Token, exchange_error: &str) -> Result<String, String> {
    let fallback = token.access_token.trim();
    if fallback.is_empty() || access_token_expires_within(fallback, 0) {
        return Err(exchange_error.to_string());
    }
    log::warn!(
        "api_key_access_token exchange unavailable; fallback to access_token: {}",
        exchange_error
    );
    Ok(fallback.to_string())
}

fn should_mark_account_unavailable_after_refresh_failure_for_bearer_exchange(
    token: &Token,
) -> bool {
    let fallback = token.access_token.trim();
    if fallback.is_empty() {
        return true;
    }

    match extract_token_exp(fallback) {
        Some(exp) => exp <= now_ts(),
        None => false,
    }
}

// 网关与后台共用 RT 刷新入口；兑换锁只合并派生 API 令牌请求，锁内重读完整凭据，避免旧快照回写。
// account/token 为同一候选账号，网络或持久化失败保留错误语义，只有仍有效的访问令牌可继续用于请求。
pub(super) fn resolve_openai_bearer_token(
    storage: &Storage,
    account: &Account,
    token: &mut Token,
) -> Result<String, String> {
    if let Some(existing) = token
        .api_key_access_token
        .as_deref()
        .and_then(usable_api_key_access_token)
    {
        return Ok(existing);
    }
    let exchange_lock = account_token_exchange_lock(&account.id);
    let _guard =
        crate::lock_utils::lock_recover(exchange_lock.as_ref(), "account_token_exchange_lock");
    *token = readLatestToken(storage, &account.id)?;
    if let Some(existing) = token
        .api_key_access_token
        .as_deref()
        .and_then(usable_api_key_access_token)
    {
        return Ok(existing);
    }
    let fallback_client_id = super::runtime_config::token_exchange_client_id();
    let client_id = api_key_exchange_client_id(token, &fallback_client_id);
    let issuer = if account.issuer.trim().is_empty() {
        super::runtime_config::token_exchange_default_issuer()
    } else {
        account.issuer.clone()
    };
    let exchange_error =
        match exchange_and_persist_api_key_access_token(storage, token, &issuer, &client_id) {
            Ok(bearer) => return Ok(bearer),
            Err(error) => error,
        };
    if token.refresh_token.trim().is_empty() {
        return fallback_to_access_token(token, &exchange_error);
    }
    let options = RefreshTokenOptions {
        issuer: &issuer,
        clientId: &fallback_client_id,
        aheadSecs: token_refresh_ahead_secs(),
    };
    match refresh_and_persist_access_token(storage, token, options) {
        Ok(()) => {
            if let Some(cached) = token
                .api_key_access_token
                .as_deref()
                .and_then(usable_api_key_access_token)
            {
                return Ok(cached);
            }
            let client_id = api_key_exchange_client_id(token, &fallback_client_id);
            if let Ok(bearer) =
                exchange_and_persist_api_key_access_token(storage, token, &issuer, &client_id)
            {
                return Ok(bearer);
            }
        }
        Err(error) => {
            if should_mark_account_unavailable_after_refresh_failure_for_bearer_exchange(token)
                && mark_account_unavailable_for_auth_error(storage, &account.id, &error)
            {
                return Err(error);
            }
            log::warn!("网关刷新令牌未完成：{}", error);
            return fallback_to_access_token(token, &error);
        }
    }
    fallback_to_access_token(token, &exchange_error)
}

/// 函数 `clear_account_token_exchange_locks_for_tests`
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
#[cfg(test)]
fn clear_account_token_exchange_locks_for_tests() {
    let lock = ACCOUNT_TOKEN_EXCHANGE_LOCKS
        .get_or_init(|| Mutex::new(AccountTokenExchangeLockTable::default()));
    if let Ok(mut table) = lock.lock() {
        table.entries.clear();
        table.last_cleanup_at = 0;
    }
}

#[cfg(test)]
#[path = "tests/token_exchange_tests.rs"]
mod tests;
