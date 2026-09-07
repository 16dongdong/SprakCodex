use std::collections::BTreeSet;

use rusqlite::{params, params_from_iter, types::Value, Result, Row};

use super::key_id_filters::{normalize_text_ids, text_id_in_clause, SQLITE_IN_CLAUSE_BATCH_SIZE};
use super::{
    now_ts, AccountQuotaCapacityOverride, AccountQuotaCapacityTemplate, QuotaSourceModelAssignment,
    Storage,
};

const QUOTA_SOURCE_MODEL_ASSIGNMENT_COLUMNS: &str =
    "source_kind, source_id, model_slug, updated_at";
const ACCOUNT_QUOTA_CAPACITY_OVERRIDE_COLUMNS: &str =
    "account_id, primary_window_tokens, secondary_window_tokens, updated_at";

impl Storage {
    pub fn list_quota_source_model_assignments(&self) -> Result<Vec<QuotaSourceModelAssignment>> {
        let mut stmt = self
            .conn
            .prepare(quota_source_model_assignments_list_sql())?;
        let rows = stmt.query_map([], map_quota_source_model_assignment_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_quota_source_model_assignments_for_kind(
        &self,
        source_kind: &str,
    ) -> Result<Vec<QuotaSourceModelAssignment>> {
        let source_kind = normalize_required_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(quota_source_model_assignments_for_kind_sql())?;
        let rows = stmt.query_map([source_kind], map_quota_source_model_assignment_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_quota_source_model_assignments_for_model(
        &self,
        source_kind: &str,
        model_slug: &str,
    ) -> Result<Vec<QuotaSourceModelAssignment>> {
        let source_kind = normalize_required_text(source_kind);
        let model_slug = normalize_required_text(model_slug);
        if source_kind.is_empty() || model_slug.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(quota_source_model_assignments_for_model_sql())?;
        let rows = stmt.query_map(
            params![source_kind, model_slug],
            map_quota_source_model_assignment_row,
        )?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_quota_source_model_assignment_targets_for_model(
        &self,
        source_kind: &str,
        model_slug: &str,
    ) -> Result<Vec<QuotaSourceModelAssignment>> {
        let source_kind = normalize_required_text(source_kind);
        let model_slug = normalize_required_text(model_slug);
        if source_kind.is_empty() || model_slug.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(quota_source_model_assignment_targets_for_model_sql())?;
        let rows = stmt.query_map(
            params![source_kind, model_slug],
            map_quota_source_model_assignment_row,
        )?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_quota_assignment_source_ids_for_kind(
        &self,
        source_kind: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_required_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(quota_assignment_source_ids_for_kind_sql())?;
        let rows = stmt.query_map([source_kind], |row| row.get(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_quota_source_model_assignments_for_sources(
        &self,
        source_kind: &str,
        source_ids: &[String],
    ) -> Result<Vec<QuotaSourceModelAssignment>> {
        let source_kind = normalize_required_text(source_kind);
        let source_ids = normalize_text_ids(source_ids);
        if source_kind.is_empty() || source_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();
        for chunk in source_ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            items.extend(list_quota_source_model_assignments_for_sources_chunk(
                self,
                &source_kind,
                chunk,
            )?);
        }
        items.sort_by(|a, b| {
            a.source_kind
                .cmp(&b.source_kind)
                .then_with(|| a.source_id.cmp(&b.source_id))
                .then_with(|| a.model_slug.cmp(&b.model_slug))
        });
        Ok(items)
    }

    pub fn list_quota_source_model_assignments_for(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(quota_assignment_models_for_source_sql())?;
        let rows = stmt.query_map(params![source_kind, source_id], |row| row.get(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn set_quota_source_model_assignments(
        &mut self,
        source_kind: &str,
        source_id: &str,
        model_slugs: &[String],
    ) -> Result<()> {
        let source_kind = normalize_required_text(source_kind);
        let source_id = normalize_required_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(());
        }

        let now = now_ts();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            delete_quota_source_model_assignments_for_source_sql(),
            params![source_kind, source_id],
        )?;
        for model_slug in normalize_model_slugs(model_slugs) {
            tx.execute(
                "INSERT INTO quota_source_model_assignments (
                    source_kind, source_id, model_slug, updated_at
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![source_kind, source_id, model_slug, now],
            )?;
        }
        tx.commit()
    }

    pub fn list_account_quota_capacity_templates(
        &self,
    ) -> Result<Vec<AccountQuotaCapacityTemplate>> {
        let mut stmt = self
            .conn
            .prepare(account_quota_capacity_templates_list_sql())?;
        let rows = stmt.query_map([], map_account_quota_capacity_template_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn upsert_account_quota_capacity_template(
        &self,
        plan_type: &str,
        primary_window_tokens: Option<i64>,
        secondary_window_tokens: Option<i64>,
    ) -> Result<()> {
        let plan_type = normalize_required_text(plan_type).to_ascii_lowercase();
        if plan_type.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO account_quota_capacity_templates (
                plan_type, primary_window_tokens, secondary_window_tokens, updated_at
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(plan_type) DO UPDATE SET
                primary_window_tokens = excluded.primary_window_tokens,
                secondary_window_tokens = excluded.secondary_window_tokens,
                updated_at = excluded.updated_at",
            params![
                plan_type,
                positive_tokens(primary_window_tokens),
                positive_tokens(secondary_window_tokens),
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn list_account_quota_capacity_overrides(
        &self,
    ) -> Result<Vec<AccountQuotaCapacityOverride>> {
        let mut stmt = self
            .conn
            .prepare(account_quota_capacity_overrides_list_sql())?;
        let rows = stmt.query_map([], map_account_quota_capacity_override_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_account_quota_capacity_overrides_for_accounts(
        &self,
        account_ids: &[String],
    ) -> Result<Vec<AccountQuotaCapacityOverride>> {
        let account_ids = normalize_text_ids(account_ids);
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();
        for chunk in account_ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            items.extend(list_account_quota_capacity_overrides_for_accounts_chunk(
                self, chunk,
            )?);
        }
        items.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        Ok(items)
    }

    pub fn upsert_account_quota_capacity_override(
        &self,
        account_id: &str,
        primary_window_tokens: Option<i64>,
        secondary_window_tokens: Option<i64>,
    ) -> Result<()> {
        let account_id = normalize_required_text(account_id);
        if account_id.is_empty() {
            return Ok(());
        }
        let primary = positive_tokens(primary_window_tokens);
        let secondary = positive_tokens(secondary_window_tokens);
        if primary.is_none() && secondary.is_none() {
            self.conn
                .execute(delete_account_quota_capacity_override_sql(), [account_id])?;
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO account_quota_capacity_overrides (
                account_id, primary_window_tokens, secondary_window_tokens, updated_at
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id) DO UPDATE SET
                primary_window_tokens = excluded.primary_window_tokens,
                secondary_window_tokens = excluded.secondary_window_tokens,
                updated_at = excluded.updated_at",
            params![account_id, primary, secondary, now_ts()],
        )?;
        Ok(())
    }

    pub(super) fn ensure_quota_pool_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS quota_source_model_assignments (
                source_kind TEXT NOT NULL,
                source_id TEXT NOT NULL,
                model_slug TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (source_kind, source_id, model_slug)
            );
            CREATE INDEX IF NOT EXISTS idx_quota_source_model_assignments_source
                ON quota_source_model_assignments(source_kind, source_id);
            CREATE INDEX IF NOT EXISTS idx_quota_source_model_assignments_model
                ON quota_source_model_assignments(model_slug, source_kind, source_id);
            CREATE TABLE IF NOT EXISTS account_quota_capacity_templates (
                plan_type TEXT PRIMARY KEY,
                primary_window_tokens INTEGER,
                secondary_window_tokens INTEGER,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS account_quota_capacity_overrides (
                account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
                primary_window_tokens INTEGER,
                secondary_window_tokens INTEGER,
                updated_at INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }
}

fn map_quota_source_model_assignment_row(row: &Row<'_>) -> Result<QuotaSourceModelAssignment> {
    Ok(QuotaSourceModelAssignment {
        source_kind: row.get(0)?,
        source_id: row.get(1)?,
        model_slug: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn quota_source_model_assignments_list_sql() -> &'static str {
    "SELECT source_kind, source_id, model_slug, updated_at
     FROM quota_source_model_assignments
     ORDER BY source_kind ASC, source_id ASC, model_slug ASC"
}

fn quota_source_model_assignments_for_kind_sql() -> &'static str {
    "SELECT source_kind, source_id, model_slug, updated_at
     FROM quota_source_model_assignments
     WHERE source_kind = ?1
     ORDER BY source_kind ASC, source_id ASC, model_slug ASC"
}

fn quota_source_model_assignments_for_model_sql() -> &'static str {
    "SELECT source_kind, source_id, model_slug, updated_at
     FROM quota_source_model_assignments
     WHERE source_kind = ?1 AND model_slug = ?2
     ORDER BY source_kind ASC, source_id ASC, model_slug ASC"
}

fn quota_source_model_assignment_targets_for_model_sql() -> &'static str {
    "SELECT
        sources.source_kind,
        sources.source_id,
        COALESCE(matches.model_slug, '') AS model_slug,
        COALESCE(matches.updated_at, 0) AS updated_at
     FROM (
        SELECT DISTINCT source_kind, source_id
        FROM quota_source_model_assignments
        WHERE source_kind = ?1
     ) sources
     LEFT JOIN quota_source_model_assignments matches
        ON matches.source_kind = sources.source_kind
       AND matches.source_id = sources.source_id
       AND matches.model_slug = ?2
     ORDER BY sources.source_kind ASC, sources.source_id ASC, model_slug ASC"
}

fn quota_assignment_source_ids_for_kind_sql() -> &'static str {
    "SELECT DISTINCT source_id
     FROM quota_source_model_assignments
     WHERE source_kind = ?1
     ORDER BY source_id ASC"
}

fn quota_assignment_models_for_source_sql() -> &'static str {
    "SELECT model_slug
     FROM quota_source_model_assignments
     WHERE source_kind = ?1 AND source_id = ?2
     ORDER BY model_slug ASC"
}

fn delete_quota_source_model_assignments_for_source_sql() -> &'static str {
    "DELETE FROM quota_source_model_assignments
     WHERE source_kind = ?1 AND source_id = ?2"
}

fn account_quota_capacity_templates_list_sql() -> &'static str {
    "SELECT plan_type, primary_window_tokens, secondary_window_tokens, updated_at
     FROM account_quota_capacity_templates
     ORDER BY plan_type ASC"
}

fn account_quota_capacity_overrides_list_sql() -> &'static str {
    "SELECT account_id, primary_window_tokens, secondary_window_tokens, updated_at
     FROM account_quota_capacity_overrides
     ORDER BY account_id ASC"
}

fn delete_account_quota_capacity_override_sql() -> &'static str {
    "DELETE FROM account_quota_capacity_overrides WHERE account_id = ?1"
}

fn list_quota_source_model_assignments_for_sources_chunk(
    storage: &Storage,
    source_kind: &str,
    source_ids: &[String],
) -> Result<Vec<QuotaSourceModelAssignment>> {
    let Some((source_condition, source_params)) = text_id_in_clause("source_id", source_ids) else {
        return Ok(Vec::new());
    };
    let sql = quota_source_model_assignments_for_sources_chunk_sql(&source_condition);
    let mut values = Vec::with_capacity(source_params.len() + 1);
    values.push(Value::Text(source_kind.to_string()));
    values.extend(source_params);
    let mut stmt = storage.conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params_from_iter(values),
        map_quota_source_model_assignment_row,
    )?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn quota_source_model_assignments_for_sources_chunk_sql(source_condition: &str) -> String {
    format!(
        "SELECT {QUOTA_SOURCE_MODEL_ASSIGNMENT_COLUMNS}
         FROM quota_source_model_assignments
         WHERE source_kind = ? AND {source_condition}"
    )
}

fn map_account_quota_capacity_template_row(row: &Row<'_>) -> Result<AccountQuotaCapacityTemplate> {
    Ok(AccountQuotaCapacityTemplate {
        plan_type: row.get(0)?,
        primary_window_tokens: row.get(1)?,
        secondary_window_tokens: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn map_account_quota_capacity_override_row(row: &Row<'_>) -> Result<AccountQuotaCapacityOverride> {
    Ok(AccountQuotaCapacityOverride {
        account_id: row.get(0)?,
        primary_window_tokens: row.get(1)?,
        secondary_window_tokens: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn list_account_quota_capacity_overrides_for_accounts_chunk(
    storage: &Storage,
    account_ids: &[String],
) -> Result<Vec<AccountQuotaCapacityOverride>> {
    let Some((condition, params)) = text_id_in_clause("account_id", account_ids) else {
        return Ok(Vec::new());
    };
    let sql = account_quota_capacity_overrides_for_accounts_chunk_sql(&condition);
    let mut stmt = storage.conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params_from_iter(params),
        map_account_quota_capacity_override_row,
    )?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn account_quota_capacity_overrides_for_accounts_chunk_sql(condition: &str) -> String {
    format!(
        "SELECT {ACCOUNT_QUOTA_CAPACITY_OVERRIDE_COLUMNS}
         FROM account_quota_capacity_overrides
         WHERE {condition}"
    )
}

fn normalize_required_text(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_model_slugs(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn positive_tokens(value: Option<i64>) -> Option<i64> {
    value.filter(|tokens| *tokens > 0)
}

#[cfg(test)]
#[path = "quota_pools_tests.rs"]
mod tests;
