use super::*;
use serde_json::{json, Value};

impl Storage {
    // 只返回已接入会话的标识，用于外部元数据的最小范围读取。
    pub fn routingSessionIds(&self) -> Result<Vec<String>> {
        self.conn.prepare("SELECT session_id FROM session_routing_bindings")?.query_map([],|r|r.get(0))?.collect()
    }
    // 标题与项目是显示缓存，不创建路由记录，不改变绑定、排序时间或聊天内容。
    pub fn syncRoutingDisplay(&self, rows: &[(String,String,String)]) -> Result<()> {
        let tx=self.conn.unchecked_transaction()?;
        for (id,title,project) in rows {
            tx.execute("UPDATE session_routing_bindings SET title=?2,project_path=?3 WHERE session_id=?1 AND (title IS NOT ?2 OR project_path IS NOT ?3)",params![id,title,project])?;
        }
        tx.commit()
    }

    // 关闭分流时也登记已识别会话，但不绑定账号；现有记录仅更新活动时间。
    pub fn observeRoutingSession(&self, sessionId: &str, source: &str, now: i64) -> Result<()> {
        self.conn.execute("INSERT INTO session_routing_bindings(session_id,route_source,status,created_at,updated_at,last_used_at,reason) VALUES(?1,?2,'unbound',?3,?3,?3,'routing_disabled') ON CONFLICT(session_id) DO UPDATE SET last_used_at=excluded.last_used_at",params![sessionId,source,now])?;
        Ok(())
    }

    // 分页只返回会话元数据，不读取令牌或聊天正文；搜索采用字面包含匹配。
    pub fn listRoutingSessions(&self, page: i64, search: &str, project: &str) -> Result<Value> {
        let page = page.max(1);
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_routing_bindings WHERE (instr(lower(session_id),lower(?1))>0 OR instr(lower(COALESCE(title,'')),lower(?1))>0) AND (?2='' OR project_path=?2)",
            params![search,project],
            |r| r.get(0),
        )?;
        let mut statement = self.conn.prepare("SELECT b.session_id,b.account_id,a.label,CASE WHEN b.account_id IS NULL AND b.status='active' THEN 'pending' ELSE b.status END,b.created_at,b.last_used_at,CASE WHEN b.account_id IS NULL AND b.status='active' THEN 'account_deleted' ELSE b.reason END,b.requested_account_id,target.label,b.title,b.project_path FROM session_routing_bindings b LEFT JOIN accounts a ON a.id=b.account_id LEFT JOIN accounts target ON target.id=b.requested_account_id WHERE (instr(lower(b.session_id),lower(?1))>0 OR instr(lower(COALESCE(b.title,'')),lower(?1))>0) AND (?3='' OR b.project_path=?3) ORDER BY b.last_used_at DESC,b.session_id LIMIT 50 OFFSET ?2")?;
        let rows = statement.query_map(params![search,(page-1).saturating_mul(50),project], |r| Ok(json!({
            "sessionId":r.get::<_,String>(0)?,"accountId":r.get::<_,Option<String>>(1)?,"accountLabel":r.get::<_,Option<String>>(2)?,
            "status":r.get::<_,String>(3)?,"createdAt":r.get::<_,i64>(4)?,"lastUsedAt":r.get::<_,i64>(5)?,"reason":r.get::<_,String>(6)?,
            "requestedAccountId":r.get::<_,Option<String>>(7)?,"requestedAccountLabel":r.get::<_,Option<String>>(8)?,"title":r.get::<_,Option<String>>(9)?,"projectPath":r.get::<_,Option<String>>(10)?,
        })))?;
        let items = rows.collect::<Result<Vec<Value>>>()?;
        let mut accounts = self.conn.prepare("SELECT id,label FROM accounts WHERE status IN ('active','force_enabled') ORDER BY sort,id")?;
        let choices = accounts
            .query_map([], |r| {
                Ok(json!({"id":r.get::<_,String>(0)?,"label":r.get::<_,String>(1)?}))
            })?
            .collect::<Result<Vec<Value>>>()?;
        let projects = self.conn.prepare("SELECT DISTINCT project_path FROM session_routing_bindings WHERE project_path IS NOT NULL AND project_path<>'' ORDER BY project_path")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>>>()?;
        Ok(json!({"items":items,"total":count,"page":page,"pageSize":50,"accounts":choices,"projects":projects}))
    }

    // 重置仅删除分流记录；已开始的响应继续，空闲连接检查发现记录缺失后重新握手。
    pub fn resetRoutingSession(&self, sessionId: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "DELETE FROM session_routing_bindings WHERE session_id=?1",
            [sessionId],
        )? != 0)
    }

    // 手动目标先校验资格再记录待切换状态；旧连接继续完成当前请求，新握手才落实新账号。
    pub fn switchRoutingSession(&self, sessionId: &str, accountId: &str, now: i64) -> Result<bool> {
        let tx = self.conn.unchecked_transaction()?;
        if selectCandidate(&tx, Some(accountId), now, (sessionId, "manual"))?.is_none() {
            return Err(rusqlite::Error::InvalidParameterName(
                "目标账号当前不可用".into(),
            ));
        }
        let changed = tx.execute("UPDATE session_routing_bindings SET requested_account_id=?2,status='pending',reason='manual_switch',updated_at=?3 WHERE session_id=?1",params![sessionId,accountId,now])?;
        tx.commit()?;
        Ok(changed != 0)
    }

    // 空闲连接校验数据库绑定和实时资格；任何重置、待切换、删除或额度耗尽都会使旧连接失效。
    pub fn routingConnectionCurrent(
        &self,
        sessionId: &str,
        accountId: &str,
        now: i64,
    ) -> Result<bool> {
        let tx = self.conn.unchecked_transaction()?;
        let current: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM session_routing_bindings WHERE session_id=?1 AND account_id=?2 AND status='active')",params![sessionId,accountId],|r|r.get(0))?;
        let valid = current
            && selectCandidate(&tx, Some(accountId), now, (sessionId, "thread-id"))?.is_some();
        tx.commit()?;
        Ok(valid)
    }

    // 只根据上游明确额度错误设置冷却；普通 429、连接错误不会误判为额度耗尽。
    pub fn recordSessionQuotaFailure(
        &self,
        accountHeader: &str,
        until: i64,
        now: i64,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO session_routing_cooldowns(account_id,until_at) SELECT id,?2 FROM accounts WHERE chatgpt_account_id=?1 OR workspace_id=?1 ON CONFLICT(account_id) DO UPDATE SET until_at=MAX(until_at,excluded.until_at)",params![accountHeader,until])?;
        tx.execute("UPDATE session_routing_bindings SET status='pending',reason='quota_exhausted',updated_at=?2 WHERE account_id IN (SELECT id FROM accounts WHERE chatgpt_account_id=?1 OR workspace_id=?1)",params![accountHeader,now])?;
        tx.commit()
    }

    // 仅更新已存在会话的活动时间，迟到响应不重新创建被用户重置的记录。
    pub fn touchRoutingSession(&self, sessionId: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE session_routing_bindings SET last_used_at=?2 WHERE session_id=?1",
            params![sessionId, now],
        )?;
        Ok(())
    }
}
