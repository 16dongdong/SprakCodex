use super::{
    AccountRoutingPreference, SessionRoutingCredential, SessionRoutingResolution, Storage,
};
use rusqlite::{params, OptionalExtension, Result};

impl Storage {
    // 分配与迁移在 IMMEDIATE 事务内完成；当前绑定有效时保持粘性，失效时才重新选择。
    pub fn resolveSessionRouting(
        &mut self,
        sessionId: &str,
        routeSource: &str,
        now: i64,
    ) -> Result<SessionRoutingResolution> {
        let sessionId = sessionId.trim();
        if sessionId.is_empty() || sessionId.len() > 512 || routeSource.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName("会话标识无效".into()));
        }
        let tx = self.conn.unchecked_transaction()?;
        let previous = tx.query_row("SELECT account_id,status,requested_account_id FROM session_routing_bindings WHERE session_id=?1", [sessionId], |row| Ok((row.get::<_,Option<String>>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?))).optional()?;
        if previous
            .as_ref()
            .is_some_and(|(account, status, _)| account.is_none() && status == "active")
        {
            tx.execute("UPDATE session_routing_bindings SET status='pending',reason='account_deleted',updated_at=?2 WHERE session_id=?1",params![sessionId,now])?;
        }
        if let Some((Some(accountId), status, _)) = &previous {
            if status == "active" {
                if let Some(credential) =
                    selectCandidate(&tx, Some(accountId), now, (sessionId, routeSource))?
                {
                    tx.execute(
                        "UPDATE session_routing_bindings SET last_used_at=?2 WHERE session_id=?1",
                        params![sessionId, now],
                    )?;
                    tx.commit()?;
                    return Ok(SessionRoutingResolution::Routed(credential));
                }
                tx.execute("UPDATE session_routing_bindings SET status='pending',reason='account_unavailable',updated_at=?2 WHERE session_id=?1",params![sessionId,now])?;
            }
        }
        let requested = previous
            .as_ref()
            .and_then(|(_, _, requested)| requested.as_deref());
        let candidate = selectCandidate(&tx, requested, now, (sessionId, routeSource))?;
        let Some(candidate) = candidate else {
            tx.execute("INSERT INTO session_routing_bindings(session_id,route_source,status,created_at,updated_at,last_used_at,reason) VALUES(?1,?2,'pending',?3,?3,?3,'no_available_account') ON CONFLICT(session_id) DO UPDATE SET status='pending',last_used_at=excluded.last_used_at",params![sessionId,routeSource,now])?;
            tx.commit()?;
            return Ok(SessionRoutingResolution::NoAvailableAccount);
        };
        tx.execute("INSERT INTO session_routing_bindings(session_id,account_id,route_source,status,created_at,updated_at,last_used_at,reason,last_account_id) VALUES(?1,?2,?3,'active',?4,?4,?4,'initial_assignment',?2) ON CONFLICT(session_id) DO UPDATE SET previous_account_id=COALESCE(session_routing_bindings.account_id,session_routing_bindings.last_account_id),last_account_id=excluded.account_id,account_id=excluded.account_id,requested_account_id=NULL,status='active',updated_at=excluded.updated_at,last_used_at=excluded.last_used_at",params![sessionId,candidate.account_id,routeSource,now])?;
        tx.commit()?;
        Ok(SessionRoutingResolution::Routed(candidate))
    }

    /// 更新单个账号是否参与新会话分配；已有绑定保留，由请求阶段返回清晰的不可用原因。
    pub fn setAccountSessionRoutingEnabled(
        &self,
        accountId: &str,
        enabled: bool,
        now: i64,
    ) -> Result<bool> {
        if !self.account_exists(accountId)? {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO account_routing_preferences (account_id, enabled, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
            params![accountId, i64::from(enabled), now],
        )?;
        Ok(true)
    }

    /// 返回账号分流开关和活跃绑定数量，供账号页一次性合并展示，避免逐账号查询。
    pub fn listAccountRoutingPreferences(&self) -> Result<Vec<AccountRoutingPreference>> {
        let mut statement = self.conn.prepare(
            "SELECT
                a.id,
                COALESCE(p.enabled, 1),
                COUNT(b.session_id)
             FROM accounts a
             LEFT JOIN account_routing_preferences p ON p.account_id = a.id
             LEFT JOIN session_routing_bindings b
               ON b.account_id = a.id AND b.status = 'active'
             GROUP BY a.id, p.enabled
             ORDER BY a.id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AccountRoutingPreference {
                account_id: row.get(0)?,
                enabled: row.get::<_, i64>(1)? != 0,
                active_binding_count: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// 返回持久化的活跃会话绑定总数，用于本机接入状态展示。
    pub fn activeSessionRoutingBindingCount(&self) -> Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM session_routing_bindings WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
    }
}

// 新分配、旧绑定复核和手动指定共用资格筛选，避免 UI 可选账号和运行时规则漂移。
fn selectCandidate(
    tx: &rusqlite::Transaction<'_>,
    accountId: Option<&str>,
    now: i64,
    route: (&str, &str),
) -> Result<Option<SessionRoutingCredential>> {
    tx.query_row("SELECT
                    a.id,
                    a.label,
                    a.chatgpt_account_id,
                    a.workspace_id,
                    t.access_token,
                    t.access_token_exp,
                    COUNT(b.session_id) AS active_binding_count
                 FROM accounts a
                 JOIN tokens t ON t.account_id = a.id
                 LEFT JOIN account_routing_preferences p ON p.account_id = a.id
                 LEFT JOIN session_routing_bindings b
                   ON b.account_id = a.id AND b.status = 'active'
                 LEFT JOIN usage_snapshots u ON u.id = (
                    SELECT latest.id
                    FROM usage_snapshots latest
                    WHERE latest.account_id = a.id
                    ORDER BY latest.captured_at DESC, latest.id DESC
                    LIMIT 1
                 )
                 WHERE a.status IN ('active', 'force_enabled')
                   AND COALESCE(p.enabled, 1) = 1
                   AND (?2 IS NULL OR a.id = ?2)
                   AND NOT EXISTS (SELECT 1 FROM session_routing_cooldowns c WHERE c.account_id=a.id AND c.until_at > ?1)
                   AND length(trim(t.access_token)) > 0
                   AND (t.access_token_exp IS NULL OR t.access_token_exp > ?1 + 60)
                   AND (u.used_percent IS NULL OR u.used_percent < 100 OR (u.resets_at IS NOT NULL AND u.resets_at <= ?1))
                   AND (u.secondary_used_percent IS NULL OR u.secondary_used_percent < 100 OR (u.secondary_resets_at IS NOT NULL AND u.secondary_resets_at <= ?1))
                 GROUP BY a.id, a.label, a.chatgpt_account_id, a.workspace_id,
                          t.access_token, t.access_token_exp, a.preferred, a.sort, a.updated_at,
                          u.used_percent, u.secondary_used_percent
                 ORDER BY active_binding_count ASC,
                          COALESCE(u.used_percent, 0) ASC,
                          COALESCE(u.secondary_used_percent, 0) ASC,
                          a.preferred DESC,
                          a.sort ASC,
                          a.updated_at DESC,
                          a.id ASC
                 LIMIT 1", params![now,accountId], |row| Ok(SessionRoutingCredential {
        session_id: route.0.into(), route_source: route.1.into(), account_id: row.get(0)?, account_label: row.get(1)?,
        chatgpt_account_id: row.get(2)?,workspace_id:row.get(3)?,access_token:row.get(4)?,active_binding_count:row.get(6)?,
    })).optional()
}
#[cfg(test)]
#[path = "sessionRoutingTests.rs"]
mod tests;

#[path = "sessionRoutingManagement.rs"]
mod management;
