use codexmanager_core::auth::{extract_client_id_claim, extract_token_exp, DEFAULT_CLIENT_ID};
use codexmanager_core::storage::{now_ts, Storage, Token};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::auth_tokens::obtain_api_key;
use crate::usage_http::{
    log_account_data_route, refresh_access_token, refresh_access_token_with_explicit_proxy,
    refresh_token_auth_error_reason_from_message, RefreshTokenAuthErrorReason,
};

pub(crate) const DEFAULT_TOKEN_REFRESH_AHEAD_SECS: i64 = 3600;
pub(crate) const ENV_TOKEN_REFRESH_AHEAD_SECS: &str = "CODEXMANAGER_TOKEN_REFRESH_AHEAD_SECS";

static TOKEN_REFRESH_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

// 刷新策略由调用方提供，区分 issuer、OAuth 客户端与提前刷新窗口，不与 API 令牌兑换的客户端混用。
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
pub(crate) struct RefreshTokenOptions<'a> {
    pub issuer: &'a str,
    pub clientId: &'a str,
    pub aheadSecs: i64,
}

// 所有 RT 轮换共用账号锁并在锁内重读快照；成功结果先原子保存，再执行可选 API 令牌兑换。
// options 描述 OAuth 端点和调度策略；网络/存储失败返回错误，版本竞争读取赢家，绝不重建已删除的账号令牌。
pub(crate) fn refresh_and_persist_access_token(
    storage: &Storage,
    token: &mut Token,
    options: RefreshTokenOptions<'_>,
) -> Result<(), String> {
    let original = token.clone();
    let refresh_lock = token_refresh_lock_for_account(&token.account_id);
    let refresh_guard = refresh_lock
        .lock()
        .map_err(|_| "刷新令牌锁已损坏".to_string())?;
    *token = readLatestToken(storage, &token.account_id)?;
    if token.access_token != original.access_token
        || token.refresh_token != original.refresh_token
        || token.id_token != original.id_token
    {
        return Ok(());
    }
    if token.refresh_token.trim().is_empty() {
        return Err("账号缺少刷新令牌，请重新授权".to_string());
    }

    let refresh_client_id = token_refresh_client_id(token, options.clientId);
    let proxy_mode = crate::account_proxy::resolve_account_proxy_mode(&token.account_id);
    log_account_data_route(
        "token_refresh",
        &token.account_id,
        &proxy_mode,
        "refresh_token",
        true,
    );
    let refreshed = match &proxy_mode {
        crate::account_proxy::AccountProxyMode::Disabled => {
            refresh_access_token(options.issuer, &refresh_client_id, &token.refresh_token)
        }
        crate::account_proxy::AccountProxyMode::Explicit { proxy_url, .. } => {
            refresh_access_token_with_explicit_proxy(
                options.issuer,
                &refresh_client_id,
                &token.refresh_token,
                proxy_url,
            )
        }
        crate::account_proxy::AccountProxyMode::Invalid { error, .. } => Err(error.clone()),
    };
    let refreshed = match refreshed {
        Ok(refreshed) => refreshed,
        Err(error) => {
            if recover_refresh_race_from_latest_token(
                storage,
                token,
                &original.refresh_token,
                &error,
            )? {
                return Ok(());
            }
            return Err(error);
        }
    };
    if refreshed.access_token.trim().is_empty()
        || refreshed
            .refresh_token
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err("刷新响应缺少有效令牌，原登录态未被覆盖".to_string());
    }
    let has_new_id_token = refreshed
        .id_token
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    let mut replacement = token.clone();
    replacement.access_token = refreshed.access_token;
    if let Some(value) = refreshed.refresh_token {
        replacement.refresh_token = value;
    }
    if let Some(value) = refreshed.id_token.filter(|value| !value.trim().is_empty()) {
        replacement.id_token = value;
    }
    replacement.last_refresh = now_ts();
    let schedule = (
        extract_token_exp(&replacement.access_token),
        next_refresh_at_from_token(&replacement, options.aheadSecs),
    );
    let replaced = storage
        .replaceTokenIfCurrent(token, &replacement, schedule)
        .map_err(|error| format!("保存刷新令牌失败：{error}"))?;
    *token = readLatestToken(storage, &token.account_id)?;
    drop(refresh_guard);
    if !replaced || !has_new_id_token {
        return Ok(());
    }

    // OAuth 轮换已经持久化；派生 API 令牌失败不应丢失已轮换的 RT，更不代表登录授权过期。
    let exchange_client_id = crate::gateway::api_key_exchange_client_id(token, &refresh_client_id);
    match obtain_api_key(options.issuer, &exchange_client_id, &token.id_token) {
        Ok(api_key) => {
            storage
                .updateApiTokenIfCurrent(token, Some(&api_key))
                .map_err(|error| format!("保存派生令牌失败：{error}"))?;
            *token = readLatestToken(storage, &token.account_id)?;
        }
        Err(_) => log::debug!("派生 API 令牌暂不可用，OAuth 刷新结果已持久化"),
    }
    Ok(())
}

// 从指定存储读取账号最新令牌；只返回完整快照，记录消失或数据库故障时明确失败，不恢复旧副本。
#[allow(non_snake_case)]
pub(crate) fn readLatestToken(storage: &Storage, accountId: &str) -> Result<Token, String> {
    storage
        .find_token_by_account_id(accountId)
        .map_err(|error| format!("读取最新令牌失败：{error}"))?
        .ok_or_else(|| "账号令牌已不存在".to_string())
}

// 判断使用或导出前是否需要主动刷新；未知有效期不能假设已更新，明确有效的 AT 则避免无意义轮换。
#[allow(non_snake_case)]
pub(crate) fn accessTokenNeedsRefresh(token: &Token, aheadSecs: i64) -> bool {
    extract_token_exp(&token.access_token)
        .map(|expiry| expiry <= now_ts().saturating_add(aheadSecs.max(0)))
        .unwrap_or(true)
}

pub(crate) fn token_refresh_ahead_secs() -> i64 {
    std::env::var(ENV_TOKEN_REFRESH_AHEAD_SECS)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_TOKEN_REFRESH_AHEAD_SECS)
}

pub(crate) fn token_refresh_client_id(token: &Token, fallback_client_id: &str) -> String {
    extract_client_id_claim(&token.access_token)
        .or_else(|| extract_client_id_claim(&token.id_token))
        .or_else(|| {
            let fallback = fallback_client_id.trim();
            (!fallback.is_empty()).then(|| fallback.to_string())
        })
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

fn token_refresh_lock_for_account(account_id: &str) -> Arc<Mutex<()>> {
    let locks = TOKEN_REFRESH_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(account_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn next_refresh_at_from_token(token: &Token, ahead_secs: i64) -> Option<i64> {
    let access_refresh_at =
        extract_token_exp(&token.access_token).map(|exp| exp.saturating_sub(ahead_secs));
    let refresh_refresh_at =
        extract_token_exp(&token.refresh_token).map(|exp| exp.saturating_sub(ahead_secs));

    match (access_refresh_at, refresh_refresh_at) {
        (Some(access_at), Some(refresh_at)) => Some(access_at.min(refresh_at)),
        (Some(access_at), None) => Some(access_at),
        (None, Some(refresh_at)) => Some(refresh_at),
        (None, None) => None,
    }
}

fn recover_refresh_race_from_latest_token(
    storage: &Storage,
    token: &mut Token,
    original_refresh_token: &str,
    err: &str,
) -> Result<bool, String> {
    if !is_refresh_race_recoverable_error(err) {
        return Ok(false);
    }

    let Some(latest) = storage
        .find_token_by_account_id(&token.account_id)
        .map_err(|err| err.to_string())?
    else {
        return Ok(false);
    };

    if latest.refresh_token.trim().is_empty() || latest.refresh_token == original_refresh_token {
        return Ok(false);
    }

    *token = latest;
    Ok(true)
}

fn is_refresh_race_recoverable_error(err: &str) -> bool {
    matches!(
        refresh_token_auth_error_reason_from_message(err),
        Some(RefreshTokenAuthErrorReason::InvalidGrant | RefreshTokenAuthErrorReason::Reused)
    )
}

#[cfg(test)]
#[path = "usage_token_refresh_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/auth/tokenLifecycleTests.rs"]
mod tokenLifecycleTests;
