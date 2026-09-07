use rusqlite::{params_from_iter, Result, Row};

use super::key_id_filters::{normalize_text_ids, text_id_in_clause, SQLITE_IN_CLAUSE_BATCH_SIZE};
use super::{now_ts, AccountSubscription, Storage};

fn account_subscription_select_columns() -> &'static str {
    "account_id, has_subscription, account_plan_type, plan_type, expires_at, renews_at, updated_at"
}

fn account_subscription_by_account_sql() -> String {
    format!(
        "SELECT {columns}
         FROM account_subscriptions
         WHERE account_id = ?1
         LIMIT 1",
        columns = account_subscription_select_columns(),
    )
}

fn account_subscription_list_sql() -> String {
    format!(
        "SELECT {columns}
         FROM account_subscriptions
         ORDER BY updated_at DESC, account_id ASC",
        columns = account_subscription_select_columns(),
    )
}

pub(super) fn delete_account_subscription_for_account_sql() -> &'static str {
    "DELETE FROM account_subscriptions WHERE account_id = ?1"
}

impl Storage {
    /// 函数 `upsert_account_subscription`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-17
    ///
    /// # 参数
    /// - self: 参数 self
    /// - account_id: 参数 account_id
    /// - has_subscription: 参数 has_subscription
    /// - account_plan_type: 参数 account_plan_type
    /// - plan_type: 参数 plan_type
    /// - expires_at: 参数 expires_at
    /// - renews_at: 参数 renews_at
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn upsert_account_subscription(
        &self,
        account_id: &str,
        has_subscription: bool,
        account_plan_type: Option<&str>,
        plan_type: Option<&str>,
        expires_at: Option<i64>,
        renews_at: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO account_subscriptions (
                account_id,
                has_subscription,
                account_plan_type,
                plan_type,
                expires_at,
                renews_at,
                updated_at
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7
            )
            ON CONFLICT(account_id) DO UPDATE SET
                has_subscription = excluded.has_subscription,
                account_plan_type = excluded.account_plan_type,
                plan_type = excluded.plan_type,
                expires_at = excluded.expires_at,
                renews_at = excluded.renews_at,
                updated_at = excluded.updated_at",
            (
                account_id,
                if has_subscription { 1 } else { 0 },
                normalize_optional_text(account_plan_type),
                normalize_optional_text(plan_type),
                expires_at,
                renews_at,
                now_ts(),
            ),
        )?;
        Ok(())
    }

    /// 函数 `delete_account_subscription`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-17
    ///
    /// # 参数
    /// - self: 参数 self
    /// - account_id: 参数 account_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn delete_account_subscription(&self, account_id: &str) -> Result<()> {
        self.conn
            .execute(delete_account_subscription_for_account_sql(), [account_id])?;
        Ok(())
    }

    /// 函数 `find_account_subscription`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-17
    ///
    /// # 参数
    /// - self: 参数 self
    /// - account_id: 参数 account_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_account_subscription(
        &self,
        account_id: &str,
    ) -> Result<Option<AccountSubscription>> {
        let mut stmt = self.conn.prepare(&account_subscription_by_account_sql())?;
        let mut rows = stmt.query([account_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_account_subscription_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `list_account_subscriptions`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-17
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_account_subscriptions(&self) -> Result<Vec<AccountSubscription>> {
        let mut stmt = self.conn.prepare(&account_subscription_list_sql())?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(map_account_subscription_row(row)?);
        }
        Ok(out)
    }

    pub fn list_account_subscriptions_for_accounts(
        &self,
        account_ids: &[String],
    ) -> Result<Vec<AccountSubscription>> {
        let account_ids = normalize_text_ids(account_ids);
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for chunk in account_ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            out.extend(list_account_subscriptions_for_accounts_chunk(self, chunk)?);
        }
        out.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        Ok(out)
    }

    /// 函数 `ensure_account_subscriptions_table`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-17
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub(super) fn ensure_account_subscriptions_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS account_subscriptions (
                account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
                has_subscription INTEGER NOT NULL DEFAULT 0,
                account_plan_type TEXT,
                plan_type TEXT,
                expires_at INTEGER,
                renews_at INTEGER,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_account_subscriptions_updated_at
                ON account_subscriptions(updated_at DESC, account_id ASC);",
        )?;
        self.ensure_column("account_subscriptions", "account_plan_type", "TEXT")?;
        Ok(())
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn map_account_subscription_row(row: &Row<'_>) -> Result<AccountSubscription> {
    Ok(AccountSubscription {
        account_id: row.get(0)?,
        has_subscription: row.get::<_, i64>(1)? != 0,
        account_plan_type: row.get(2)?,
        plan_type: row.get(3)?,
        expires_at: row.get(4)?,
        renews_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn list_account_subscriptions_for_accounts_chunk(
    storage: &Storage,
    account_ids: &[String],
) -> Result<Vec<AccountSubscription>> {
    let Some((condition, params)) = text_id_in_clause("account_id", account_ids) else {
        return Ok(Vec::new());
    };
    let sql = account_subscriptions_for_accounts_chunk_sql(&condition);
    let mut stmt = storage.conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_account_subscription_row(row)?);
    }
    Ok(out)
}

fn account_subscriptions_for_accounts_chunk_sql(account_condition: &str) -> String {
    format!(
        "SELECT {columns}
         FROM account_subscriptions
         WHERE {account_condition}",
        columns = account_subscription_select_columns(),
    )
}

#[cfg(test)]
#[path = "account_subscriptions_tests.rs"]
mod tests;
