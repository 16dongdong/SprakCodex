use rusqlite::{params, OptionalExtension, Result};

use super::{
    AccountRoutingPreference, SessionRoutingCredential, SessionRoutingResolution, Storage,
};

const ACTIVE_STATUSES: &[&str] = &["active", "force_enabled"];

struct BoundAccountSnapshot {
    accountId: String,
    accountLabel: Option<String>,
    chatgptAccountId: Option<String>,
    workspaceId: Option<String>,
    accessToken: Option<String>,
    accessTokenExpiresAt: Option<i64>,
    accountStatus: Option<String>,
    routingEnabled: bool,
    activeBindingCount: i64,
}

impl BoundAccountSnapshot {
    /// 将事务内读取的账号快照转换为一次不可变的路由凭据；失败原因只描述状态，不携带令牌内容。
    fn intoResolution(
        self,
        sessionId: &str,
        routeSource: &str,
        now: i64,
    ) -> SessionRoutingResolution {
        let unavailable = |reason: &str| SessionRoutingResolution::BoundAccountUnavailable {
            account_id: self.accountId.clone(),
            reason: reason.to_string(),
        };
        if !self.routingEnabled {
            return unavailable("account_routing_disabled");
        }
        if !self
            .accountStatus
            .as_deref()
            .is_some_and(|status| ACTIVE_STATUSES.contains(&status))
        {
            return unavailable("account_unavailable");
        }
        if self
            .accessTokenExpiresAt
            .is_some_and(|expiresAt| expiresAt <= now + 60)
        {
            return unavailable("access_token_expired");
        }
        let Some(accessToken) = self.accessToken.filter(|value| !value.trim().is_empty()) else {
            return unavailable("access_token_missing");
        };
        let Some(accountLabel) = self.accountLabel else {
            return unavailable("account_missing");
        };
        SessionRoutingResolution::Routed(SessionRoutingCredential {
            session_id: sessionId.to_string(),
            route_source: routeSource.to_string(),
            account_id: self.accountId,
            account_label: accountLabel,
            chatgpt_account_id: self.chatgptAccountId,
            workspace_id: self.workspaceId,
            access_token: accessToken,
            active_binding_count: self.activeBindingCount,
        })
    }
}

impl Storage {
    /// 为新会话原子选择负载最低的可用账号，已有会话只返回原绑定；绑定账号失效时返回明确状态，绝不静默换号。
    pub fn resolveSessionRouting(
        &mut self,
        sessionId: &str,
        routeSource: &str,
        now: i64,
    ) -> Result<SessionRoutingResolution> {
        let normalizedSessionId = sessionId.trim();
        let normalizedRouteSource = routeSource.trim();
        if normalizedSessionId.is_empty() || normalizedRouteSource.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "session id and route source are required".to_string(),
            ));
        }

        // IMMEDIATE 事务把“读取负载、选账号、写绑定”固定为一个临界区，避免并发首请求重复分配。
        let transaction = self.conn.unchecked_transaction()?;
        let existingAccountId = transaction
            .query_row(
                "SELECT account_id
                 FROM session_routing_bindings
                 WHERE session_id = ?1 AND status = 'active'
                 LIMIT 1",
                [normalizedSessionId],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if let Some(accountId) = existingAccountId {
            let snapshot = readBoundAccountSnapshot(&transaction, &accountId)?;
            transaction.execute(
                "UPDATE session_routing_bindings
                 SET updated_at = ?1, last_used_at = ?1
                 WHERE session_id = ?2",
                params![now, normalizedSessionId],
            )?;
            transaction.commit()?;
            return Ok(snapshot.intoResolution(normalizedSessionId, normalizedRouteSource, now));
        }

        let candidate = transaction
            .query_row(
                "SELECT
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
                   AND length(trim(t.access_token)) > 0
                   AND (t.access_token_exp IS NULL OR t.access_token_exp > ?1 + 60)
                   AND (u.used_percent IS NULL OR u.used_percent < 100 OR COALESCE(u.resets_at, 0) <= ?1)
                   AND (u.secondary_used_percent IS NULL OR u.secondary_used_percent < 100 OR COALESCE(u.secondary_resets_at, 0) <= ?1)
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
                 LIMIT 1",
                [now],
                |row| {
                    Ok(SessionRoutingCredential {
                        session_id: normalizedSessionId.to_string(),
                        route_source: normalizedRouteSource.to_string(),
                        account_id: row.get(0)?,
                        account_label: row.get(1)?,
                        chatgpt_account_id: row.get(2)?,
                        workspace_id: row.get(3)?,
                        access_token: row.get(4)?,
                        active_binding_count: row.get(6)?,
                    })
                },
            )
            .optional()?;
        let Some(candidate) = candidate else {
            transaction.commit()?;
            return Ok(SessionRoutingResolution::NoAvailableAccount);
        };
        transaction.execute(
            "INSERT INTO session_routing_bindings (
                session_id, account_id, route_source, status,
                created_at, updated_at, last_used_at
             ) VALUES (?1, ?2, ?3, 'active', ?4, ?4, ?4)",
            params![
                normalizedSessionId,
                candidate.account_id,
                normalizedRouteSource,
                now
            ],
        )?;
        transaction.commit()?;
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

/// 在同一事务中读取已绑定账号及凭据状态；LEFT JOIN 保留被删令牌等异常，确保返回具体失败语义。
fn readBoundAccountSnapshot(
    transaction: &rusqlite::Transaction<'_>,
    accountId: &str,
) -> Result<BoundAccountSnapshot> {
    transaction.query_row(
        "SELECT
            b.account_id,
            a.label,
            a.chatgpt_account_id,
            a.workspace_id,
            t.access_token,
            t.access_token_exp,
            a.status,
            COALESCE(p.enabled, 1),
            (SELECT COUNT(*) FROM session_routing_bindings active
             WHERE active.account_id = b.account_id AND active.status = 'active')
         FROM session_routing_bindings b
         LEFT JOIN accounts a ON a.id = b.account_id
         LEFT JOIN tokens t ON t.account_id = b.account_id
         LEFT JOIN account_routing_preferences p ON p.account_id = b.account_id
         WHERE b.account_id = ?1
         LIMIT 1",
        [accountId],
        |row| {
            Ok(BoundAccountSnapshot {
                accountId: row.get(0)?,
                accountLabel: row.get(1)?,
                chatgptAccountId: row.get(2)?,
                workspaceId: row.get(3)?,
                accessToken: row.get(4)?,
                accessTokenExpiresAt: row.get(5)?,
                accountStatus: row.get(6)?,
                routingEnabled: row.get::<_, i64>(7)? != 0,
                activeBindingCount: row.get(8)?,
            })
        },
    )
}

#[cfg(test)]
#[path = "sessionRoutingTests.rs"]
mod tests;
