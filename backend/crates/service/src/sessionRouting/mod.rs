mod metadata;
use codexmanager_core::storage::{
    now_ts, AccountRoutingPreference, SessionRoutingCredential, SessionRoutingResolution, Storage,
};
use hyper::{header, header::HeaderValue, HeaderMap};
use serde::Serialize;
use std::path::PathBuf;

pub const enabledSettingKey: &str = "sessionRouting.enabled";

/// 个人版不提供旧平台网关；该运行时常量没有环境变量或数据库覆盖入口。
pub(crate) fn legacyGatewayEnabled() -> bool {
    false
}

#[derive(Debug, Clone)]
pub(crate) enum RouteDecision {
    Passthrough {
        reason: &'static str,
        sessionId: Option<String>,
        routeSource: Option<String>,
    },
    Routed(SessionRoutingCredential),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRoutingStatus {
    enabled: bool,
    active_binding_count: i64,
    accounts: Vec<AccountRoutingPreferenceStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountRoutingPreferenceStatus {
    account_id: String,
    enabled: bool,
    active_binding_count: i64,
}

/// 读取全局开关和账号绑定摘要；数据库错误直接返回，避免界面把未知状态显示成关闭。
pub fn status() -> Result<SessionRoutingStatus, String> {
    let storage = openStorage()?;
    let enabled = readEnabled(&storage)?;
    let accounts = storage
        .listAccountRoutingPreferences()
        .map_err(|error| format!("读取账号分流状态失败：{error}"))?
        .into_iter()
        .map(accountPreferenceStatus)
        .collect();
    let activeBindingCount = storage
        .activeSessionRoutingBindingCount()
        .map_err(|error| format!("读取会话绑定数量失败：{error}"))?;
    Ok(SessionRoutingStatus {
        enabled,
        active_binding_count: activeBindingCount,
        accounts,
    })
}

/// 持久化会话分流总开关；开关只影响后续请求，不修改 Codex 配置或正在传输的连接。
pub fn setEnabled(enabled: bool) -> Result<SessionRoutingStatus, String> {
    let storage = openStorage()?;
    storage
        .set_app_setting(
            enabledSettingKey,
            if enabled { "true" } else { "false" },
            now_ts(),
        )
        .map_err(|error| format!("保存会话分流开关失败：{error}"))?;
    status()
}

/// 更新账号是否参与新会话分配；返回完整摘要，使前端不依赖乐观状态猜测。
pub fn setAccountEnabled(accountId: &str, enabled: bool) -> Result<SessionRoutingStatus, String> {
    let storage = openStorage()?;
    let updated = storage
        .setAccountSessionRoutingEnabled(accountId.trim(), enabled, now_ts())
        .map_err(|error| format!("保存账号分流状态失败：{error}"))?;
    if !updated {
        return Err("账号不存在".to_string());
    }
    status()
}

/// 在网络请求进入上游前读取全局开关并原子解析会话绑定；凭据只存在于本次返回快照中。
pub(crate) async fn resolveRequest(headers: &HeaderMap) -> Result<RouteDecision, String> {
    let Some((sessionId, routeSource)) = routeIdentity(headers) else {
        return Ok(RouteDecision::Passthrough {
            reason: "missing_stable_session_id",
            sessionId: None,
            routeSource: None,
        });
    };
    let databasePath = crate::process_env::ensure_default_db_path();
    tokio::task::spawn_blocking(move || resolveStoredRoute(databasePath, sessionId, routeSource))
        .await
        .map_err(|_| "会话分流任务异常退出".to_string())?
}

/// 将已选账号作为一个身份快照写入请求头；缺少账号标识时拒绝混用客户端原身份。
pub(crate) fn applyDecision(
    headers: &mut HeaderMap,
    decision: &RouteDecision,
) -> Result<(), String> {
    let RouteDecision::Routed(credential) = decision else {
        return Ok(());
    };
    let accountIdentity = credential
        .chatgpt_account_id
        .as_deref()
        .or(credential.workspace_id.as_deref())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "分流账号缺少 ChatGPT 账号标识".to_string())?;
    let authorization = HeaderValue::from_str(&format!("Bearer {}", credential.access_token))
        .map_err(|_| "分流账号访问令牌格式无效".to_string())?;
    let accountHeader =
        HeaderValue::from_str(accountIdentity).map_err(|_| "分流账号标识格式无效".to_string())?;
    headers.insert(header::AUTHORIZATION, authorization);
    headers.insert("chatgpt-account-id", accountHeader);
    Ok(())
}

/// 返回日志使用的稳定路由元数据，不包含访问令牌或可逆凭据。
pub(crate) fn decisionMetadata(
    decision: &RouteDecision,
) -> (&'static str, Option<&str>, Option<&str>, &'static str) {
    match decision {
        RouteDecision::Passthrough {
            reason,
            sessionId,
            routeSource,
        } => (
            "passthrough",
            sessionId.as_deref(),
            routeSource.as_deref(),
            reason,
        ),
        RouteDecision::Routed(credential) => (
            "sessionRouting",
            Some(credential.session_id.as_str()),
            Some(credential.route_source.as_str()),
            "bound_account",
        ),
    }
}

/// 已路由请求使用账号池中的显示名称；令牌声明缺失时仍能准确显示实际账号。
pub(crate) fn routedAccountLabel(decision: &RouteDecision) -> Option<&str> {
    match decision {
        RouteDecision::Routed(credential) => Some(credential.account_label.as_str()),
        RouteDecision::Passthrough { .. } => None,
    }
}

/// 在阻塞线程中完成设置读取和绑定事务；参数均为已规范化的会话标识，失败时不改变请求身份。
fn resolveStoredRoute(
    databasePath: PathBuf,
    sessionId: String,
    routeSource: String,
) -> Result<RouteDecision, String> {
    let mut storage =
        Storage::open(databasePath).map_err(|error| format!("打开分流数据库失败：{error}"))?;
    if !readEnabled(&storage)? {
        storage.observeRoutingSession(&sessionId,&routeSource,now_ts()).map_err(|e|e.to_string())?;
        return Ok(RouteDecision::Passthrough {
            reason: "routing_disabled",
            sessionId: Some(sessionId),
            routeSource: Some(routeSource),
        });
    }
    match storage
        .resolveSessionRouting(&sessionId, &routeSource, now_ts())
        .map_err(|error| format!("解析会话绑定失败：{error}"))?
    {
        SessionRoutingResolution::Routed(credential) => Ok(RouteDecision::Routed(credential)),
        SessionRoutingResolution::NoAvailableAccount => Err("会话分流没有可用账号".to_string()),
        SessionRoutingResolution::BoundAccountUnavailable { account_id, reason } => Err(format!(
            "会话绑定账号当前不可用：accountId={account_id} reason={reason}"
        )),
    }
}

/// 从持久化设置读取总开关；缺少记录等价于关闭，确保升级后的首次启动保持纯透传。
fn readEnabled(storage: &Storage) -> Result<bool, String> {
    storage
        .get_app_setting(enabledSettingKey)
        .map_err(|error| format!("读取会话分流开关失败：{error}"))
        .map(|value| value.is_some_and(|value| value.eq_ignore_ascii_case("true")))
}

/// 复用服务统一数据库连接池；连接池尚未初始化时返回明确错误，不创建第二套存储生命周期。
fn openStorage() -> Result<crate::storage_helpers::StorageHandle, String> {
    crate::storage_helpers::open_storage().ok_or_else(|| "分流数据库不可用".to_string())
}

/// 将存储层字段映射为稳定的前端响应结构，避免把数据库命名约定泄漏到 RPC 协议。
fn accountPreferenceStatus(value: AccountRoutingPreference) -> AccountRoutingPreferenceStatus {
    AccountRoutingPreferenceStatus {
        account_id: value.account_id,
        enabled: value.enabled,
        active_binding_count: value.active_binding_count,
    }
}

/// 只接受当前真实客户端已经稳定发送的会话头；窗口标识不代表会话，不能作为绑定键。
fn routeIdentity(headers: &HeaderMap) -> Option<(String, String)> {
    for name in ["thread-id", "session-id"] {
        let value = headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 512)
            .filter(|value| !value.chars().any(char::is_control));
        if let Some(value) = value {
            return Some((value.to_string(), name.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证关闭分流时认证字段逐值保留；该边界防止观察链路意外改变客户端身份。
    #[test]
    fn passthroughPreservesIdentityHeaders() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer SOURCE_TOKEN"),
        );
        headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_static("SOURCE_ACCOUNT"),
        );
        applyDecision(
            &mut headers,
            &RouteDecision::Passthrough {
                reason: "routing_disabled",
                sessionId: Some("thread-a".to_string()),
                routeSource: Some("thread-id".to_string()),
            },
        )
        .expect("应用透传决策");
        assert_eq!(headers[header::AUTHORIZATION], "Bearer SOURCE_TOKEN");
        assert_eq!(headers["chatgpt-account-id"], "SOURCE_ACCOUNT");
    }

    /// 验证会话头优先级固定为 thread-id，再退回 session-id，避免同一会话产生两套绑定。
    #[test]
    fn threadIdentityHasPriority() {
        let mut headers = HeaderMap::new();
        headers.insert("thread-id", HeaderValue::from_static("thread-a"));
        headers.insert("session-id", HeaderValue::from_static("session-a"));
        assert_eq!(
            routeIdentity(&headers),
            Some(("thread-a".to_string(), "thread-id".to_string()))
        );
    }

    /// 验证路由决策同时替换访问令牌和账号标识，不保留客户端旧身份的任一半。
    #[test]
    fn routedDecisionReplacesIdentityAtomically() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer SOURCE_TOKEN"),
        );
        headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_static("SOURCE_ACCOUNT"),
        );
        let decision = RouteDecision::Routed(SessionRoutingCredential {
            session_id: "thread-a".to_string(),
            route_source: "thread-id".to_string(),
            account_id: "account-a".to_string(),
            account_label: "账号 A".to_string(),
            chatgpt_account_id: Some("TARGET_ACCOUNT".to_string()),
            workspace_id: None,
            access_token: "TARGET_TOKEN".to_string(),
            active_binding_count: 1,
        });
        applyDecision(&mut headers, &decision).expect("应用分流身份");
        assert_eq!(headers[header::AUTHORIZATION], "Bearer TARGET_TOKEN");
        assert_eq!(headers["chatgpt-account-id"], "TARGET_ACCOUNT");
    }

    /// 验证发布构建没有重新开放旧平台网关的配置入口。
    #[test]
    fn legacyGatewayRemainsDisabled() {
        assert!(!legacyGatewayEnabled());
    }
}

// 会话页查询和写操作保留服务数据库错误；标识长度在 RPC 边界统一限制。
pub fn listSessions(page: i64, search: &str, project: &str) -> Result<serde_json::Value,String> {
    if search.len()>512 { return Err("搜索内容过长".into()); }
    let storage=openStorage()?;
    let warning=metadata::sync(&storage).err();
    let mut result=storage.listRoutingSessions(page,search,project).map_err(|e|e.to_string())?;
    result["metadataWarning"]=serde_json::json!(warning);
    Ok(result)
}
// 重置不删除 Codex 内容，也不创建替代会话；下次连接由分配事务重新处理。
pub fn resetSession(sessionId: &str) -> Result<serde_json::Value,String> {
    validateSessionId(sessionId)?;
    let changed = openStorage()?.resetRoutingSession(sessionId).map_err(|e|e.to_string())?;
    Ok(serde_json::json!({"ok":changed}))
}
// 手动换号记录待生效状态，不改变正在发送的请求凭据。
pub fn switchSession(sessionId: &str, accountId: &str) -> Result<serde_json::Value,String> {
    validateSessionId(sessionId)?;
    let changed = openStorage()?.switchRoutingSession(sessionId,accountId,now_ts()).map_err(|e|e.to_string())?;
    Ok(serde_json::json!({"ok":changed}))
}
// 拒绝空标识、过长文本和控制字符，避免管理操作写入不可重现的绑定键。
fn validateSessionId(sessionId: &str) -> Result<(),String> {
    if sessionId.is_empty() || sessionId.len()>512 || sessionId.chars().any(char::is_control) { return Err("会话标识无效".into()); }
    Ok(())
}
// WebSocket 已开始响应时不调用此检查；空闲时发现绑定变化则关闭旧连接，让客户端重新握手。
pub(crate) async fn connectionCurrent(decision: &RouteDecision) -> Result<bool,String> {
    let RouteDecision::Routed(credential) = decision else {
        if matches!(decision,RouteDecision::Passthrough { reason: "routing_disabled", .. }) {
            return tokio::task::spawn_blocking(|| { let storage=openStorage()?; Ok(!readEnabled(&storage)?) }).await.map_err(|e|e.to_string())?;
        }
        return Ok(true);
    };
    let session = credential.session_id.clone();
    let account = credential.account_id.clone();
    tokio::task::spawn_blocking(move || {
        let storage = openStorage()?;
        if !readEnabled(&storage)? { return Ok(false); }
        storage.routingConnectionCurrent(&session,&account,now_ts()).map_err(|e|e.to_string())
    }).await.map_err(|e|e.to_string())?
}
// 没有重置时间的明确额度拒绝只短暂冷却五分钟；不以普通限速响应推断账号额度。
pub(crate) fn quotaCooldown(diagnostic: &str, now: i64) -> Option<i64> {
    let error: serde_json::Value = serde_json::from_str(diagnostic).ok()?;
    let code = error.get("code").or_else(|| error.get("type"))?.as_str()?;
    if !matches!(code,"usage_limit_reached"|"insufficient_quota"|"quota_exceeded") { return None; }
    Some(error.get("resets_at").and_then(|v|v.as_i64()).filter(|v|*v>now).unwrap_or(now+300))
}
