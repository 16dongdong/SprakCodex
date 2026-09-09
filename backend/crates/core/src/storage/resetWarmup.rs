use super::{Storage, UsageSnapshotRecord};
use rusqlite::{params, OptionalExtension, Result};

impl Storage {
    // 仅记录服务端声明的主额度周期；已到期但未领取的任务保留，避免刷新覆盖待执行重置。
    // 每个账号最多两条周期记录，claimed_at 随数据库持久化，重启不会重复预热同一周期。
    pub fn observeResetWarmup(&self, snapshot: &UsageSnapshotRecord) -> Result<()> {
        for (minutes, deadline) in [
            (snapshot.window_minutes, snapshot.resets_at),
            (snapshot.secondary_window_minutes, snapshot.secondary_resets_at),
        ] {
            if let (Some(minutes @ (300 | 10080)), Some(deadline)) = (minutes, deadline) {
                if deadline <= 0 { continue; }
                self.conn.execute(
                    "INSERT INTO quota_reset_warmup(account_id,window_minutes,reset_at) VALUES (?1,?2,?3)
                     ON CONFLICT(account_id,window_minutes) DO UPDATE SET reset_at=excluded.reset_at,claimed_at=NULL
                     WHERE excluded.reset_at>quota_reset_warmup.reset_at
                     AND (quota_reset_warmup.claimed_at IS NOT NULL OR quota_reset_warmup.reset_at>?4)",
                    params![snapshot.account_id, minutes, deadline, snapshot.captured_at],
                )?;
            }
        }
        Ok(())
    }

    // 只有确认兑换成功才写入立即任务；此入口不执行兑换，且同一秒的重复回调不会清除领取状态。
    pub fn enqueueResetWarmup(&self, accountId: &str, now: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        // 手动兑换使旧周期失效；清除旧周期任务，防止其截止时间再触发一次无意义预热。
        tx.execute("UPDATE quota_reset_warmup SET claimed_at=?2 WHERE account_id=?1 AND window_minutes<>0", params![accountId, now])?;
        tx.execute(
            "INSERT INTO quota_reset_warmup(account_id,window_minutes,reset_at) VALUES (?1,0,?2)
             ON CONFLICT(account_id,window_minutes) DO UPDATE SET reset_at=excluded.reset_at,claimed_at=NULL
             WHERE excluded.reset_at>quota_reset_warmup.reset_at",
            params![accountId, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    // 事务内先领取再发网络请求，跨进程争抢只产生一个赢家；同时到期的周期合并为一次账号预热。
    // 请求结果不明确时不自动重发，避免重复计费；失败通过现有预热日志显示，可由用户手动重试。
    pub fn claimResetWarmup(&mut self, now: i64) -> Result<Option<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let accountId = tx.query_row(
            "SELECT q.account_id FROM quota_reset_warmup q JOIN accounts a ON a.id=q.account_id
             WHERE q.claimed_at IS NULL AND q.reset_at<=?1 AND a.status IN ('active','available','limited')
             ORDER BY q.reset_at,q.account_id LIMIT 1",
            [now], |row| row.get::<_, String>(0),
        ).optional()?;
        if let Some(accountId) = &accountId {
            let changed = tx.execute(
                "UPDATE quota_reset_warmup SET claimed_at=?2 WHERE account_id=?1 AND reset_at<=?2 AND claimed_at IS NULL",
                params![accountId, now],
            )?;
            if changed == 0 { return Ok(None); }
        }
        tx.commit()?;
        Ok(accountId)
    }
}
