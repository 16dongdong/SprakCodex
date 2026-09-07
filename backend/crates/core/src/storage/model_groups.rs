use rusqlite::{params, OptionalExtension, Result, Row};

use super::{
    now_ts, ModelGroup, ModelGroupAccess, ModelGroupListSnapshot, ModelGroupModel, Storage,
    UserModelGroup,
};

const DEFAULT_MODEL_GROUP_ID: &str = "mg_default";

fn map_model_group(row: &Row<'_>) -> Result<ModelGroup> {
    Ok(ModelGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        sort: row.get(4)?,
        is_default: row.get::<_, i64>(5)? != 0,
        rate_multiplier_millis: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_model_group_model(row: &Row<'_>) -> Result<ModelGroupModel> {
    Ok(ModelGroupModel {
        group_id: row.get(0)?,
        platform_model_slug: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        rate_multiplier_millis: row.get(3)?,
        billing_model_slug: row.get(4)?,
        note: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_user_model_group(row: &Row<'_>) -> Result<UserModelGroup> {
    Ok(UserModelGroup {
        user_id: row.get(0)?,
        group_id: row.get(1)?,
        status: row.get(2)?,
        expires_at: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn model_group_list_sql() -> &'static str {
    "SELECT id, name, description, status, sort, is_default,
            rate_multiplier_millis, created_at, updated_at
     FROM model_groups
     ORDER BY sort ASC, name ASC, created_at ASC"
}

fn model_group_models_list_sql() -> &'static str {
    "SELECT group_id, platform_model_slug, enabled, rate_multiplier_millis,
            billing_model_slug, note, created_at, updated_at
     FROM model_group_models
     ORDER BY group_id ASC, platform_model_slug ASC"
}

fn model_group_by_id_sql() -> &'static str {
    "SELECT id, name, description, status, sort, is_default,
            rate_multiplier_millis, created_at, updated_at
     FROM model_groups
     WHERE id = ?1"
}

fn model_group_models_for_group_sql() -> &'static str {
    "SELECT group_id, platform_model_slug, enabled, rate_multiplier_millis,
            billing_model_slug, note, created_at, updated_at
     FROM model_group_models
     WHERE group_id = ?1
     ORDER BY platform_model_slug ASC"
}

fn user_model_groups_list_sql() -> &'static str {
    "SELECT user_id, group_id, status, expires_at, created_at, updated_at
     FROM user_model_groups
     ORDER BY user_id ASC, group_id ASC"
}

fn user_model_groups_for_user_sql() -> &'static str {
    "SELECT user_id, group_id, status, expires_at, created_at, updated_at
     FROM user_model_groups
     WHERE user_id = ?1
     ORDER BY group_id ASC"
}

fn delete_user_model_groups_for_group_sql() -> &'static str {
    "DELETE FROM user_model_groups WHERE group_id = ?1"
}

fn delete_model_group_models_for_platform_model_sql() -> &'static str {
    "DELETE FROM model_group_models WHERE platform_model_slug = ?1"
}

fn clear_other_default_model_groups_sql() -> &'static str {
    "UPDATE model_groups SET is_default = 0 WHERE id <> ?1"
}

fn default_model_group_id_sql() -> &'static str {
    "SELECT id FROM model_groups WHERE is_default = 1 LIMIT 1"
}

fn delete_non_default_model_group_by_id_sql() -> &'static str {
    "DELETE FROM model_groups WHERE id = ?1 AND is_default = 0"
}

fn delete_model_group_models_for_group_sql() -> &'static str {
    "DELETE FROM model_group_models WHERE group_id = ?1"
}

fn prune_default_model_group_models_not_in_catalog_sql() -> &'static str {
    "DELETE FROM model_group_models
     WHERE group_id IN (SELECT id FROM model_groups WHERE is_default = 1)
       AND EXISTS (
           SELECT 1
           FROM model_catalog_models
           WHERE scope = 'default'
             AND COALESCE(supported_in_api, 1) = 1
             AND TRIM(slug) <> ''
       )
       AND platform_model_slug NOT IN (
           SELECT slug
           FROM model_catalog_models
           WHERE scope = 'default'
             AND COALESCE(supported_in_api, 1) = 1
             AND TRIM(slug) <> ''
       )"
}

fn allowed_model_slugs_for_user_sql() -> &'static str {
    "SELECT DISTINCT m.platform_model_slug
     FROM user_model_groups u
     JOIN model_groups g ON g.id = u.group_id
     JOIN model_group_models m ON m.group_id = g.id
     JOIN model_catalog_models c
       ON c.scope = 'default'
      AND c.slug = m.platform_model_slug
      AND COALESCE(c.supported_in_api, 1) = 1
      AND TRIM(c.slug) <> ''
     WHERE u.user_id = ?1
       AND u.status = 'active'
       AND (u.expires_at IS NULL OR u.expires_at > ?2)
       AND g.status = 'active'
       AND m.enabled = 1
     ORDER BY m.platform_model_slug ASC"
}

fn model_group_access_for_user_sql() -> &'static str {
    "SELECT g.id, g.name, m.platform_model_slug, g.rate_multiplier_millis,
            m.rate_multiplier_millis, m.billing_model_slug
     FROM user_model_groups u
     JOIN model_groups g ON g.id = u.group_id
     JOIN model_group_models m ON m.group_id = g.id
     JOIN model_catalog_models c
       ON c.scope = 'default'
      AND c.slug = m.platform_model_slug
      AND COALESCE(c.supported_in_api, 1) = 1
      AND TRIM(c.slug) <> ''
     WHERE u.user_id = ?1
       AND m.platform_model_slug = ?2
       AND u.status = 'active'
       AND (u.expires_at IS NULL OR u.expires_at > ?3)
       AND g.status = 'active'
       AND m.enabled = 1"
}
impl Storage {
    pub(super) fn ensure_model_group_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
                sort INTEGER NOT NULL DEFAULT 0,
                is_default INTEGER NOT NULL DEFAULT 0,
                rate_multiplier_millis INTEGER NOT NULL DEFAULT 1000,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_model_groups_status_sort
                ON model_groups(status, sort, name);
            CREATE INDEX IF NOT EXISTS idx_model_groups_list_order
                ON model_groups(sort ASC, name ASC, created_at ASC, id ASC);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_model_groups_default
                ON model_groups(is_default)
                WHERE is_default = 1;
            CREATE TABLE IF NOT EXISTS model_group_models (
                group_id TEXT NOT NULL REFERENCES model_groups(id) ON DELETE CASCADE,
                platform_model_slug TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                rate_multiplier_millis INTEGER,
                billing_model_slug TEXT,
                note TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (group_id, platform_model_slug)
            );
            CREATE INDEX IF NOT EXISTS idx_model_group_models_model
                ON model_group_models(platform_model_slug, enabled);
            CREATE TABLE IF NOT EXISTS user_model_groups (
                user_id TEXT NOT NULL REFERENCES app_users(id) ON DELETE CASCADE,
                group_id TEXT NOT NULL REFERENCES model_groups(id) ON DELETE CASCADE,
                status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
                expires_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (user_id, group_id)
            );
            CREATE INDEX IF NOT EXISTS idx_user_model_groups_user_status
                ON user_model_groups(user_id, status, expires_at);
            CREATE INDEX IF NOT EXISTS idx_user_model_groups_group_lookup
                ON user_model_groups(group_id, user_id);",
        )?;
        self.bootstrap_default_model_group()?;
        Ok(())
    }

    pub fn bootstrap_default_model_group(&self) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT OR IGNORE INTO model_groups (
                id, name, description, status, sort, is_default,
                rate_multiplier_millis, created_at, updated_at
             ) VALUES (?1, '默认模型组', '升级前已有用户的默认可用模型集合',
                'active', 0, 1, 1000, ?2, ?2)",
            params![DEFAULT_MODEL_GROUP_ID, now],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO model_group_models (
                group_id, platform_model_slug, enabled, rate_multiplier_millis,
                billing_model_slug, note, created_at, updated_at
             )
             SELECT ?1, slug, 1, NULL, NULL, 'bootstrap', ?2, ?2
             FROM model_catalog_models
             WHERE scope = 'default'
               AND COALESCE(supported_in_api, 1) = 1
               AND TRIM(slug) <> ''",
            params![DEFAULT_MODEL_GROUP_ID, now],
        )?;
        self.prune_default_model_group_models_not_in_catalog()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO user_model_groups (
                user_id, group_id, status, expires_at, created_at, updated_at
             )
             SELECT id, ?1, 'active', NULL, ?2, ?2
             FROM app_users
             WHERE role = 'member'
               AND status = 'active'",
            params![DEFAULT_MODEL_GROUP_ID, now],
        )?;
        Ok(())
    }

    pub fn prune_default_model_group_models_not_in_catalog(&self) -> Result<()> {
        self.conn
            .execute(prune_default_model_group_models_not_in_catalog_sql(), [])?;
        Ok(())
    }

    pub fn delete_model_group_model_references(&self, platform_model_slug: &str) -> Result<()> {
        let slug = platform_model_slug.trim();
        if slug.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            delete_model_group_models_for_platform_model_sql(),
            params![slug],
        )?;
        Ok(())
    }

    pub fn default_model_group_id(&self) -> Result<Option<String>> {
        self.conn
            .query_row(default_model_group_id_sql(), [], |row| row.get(0))
            .optional()
    }

    pub fn list_model_groups(&self) -> Result<Vec<ModelGroup>> {
        let mut stmt = self.conn.prepare(model_group_list_sql())?;
        let rows = stmt.query_map([], map_model_group)?;
        rows.collect()
    }

    pub fn load_model_group_list_snapshot(&self) -> Result<ModelGroupListSnapshot> {
        self.bootstrap_default_model_group()?;
        Ok(ModelGroupListSnapshot {
            groups: self.list_model_groups()?,
            models: self.list_model_group_models()?,
            user_assignments: self.list_user_model_groups()?,
        })
    }

    pub fn find_model_group(&self, id: &str) -> Result<Option<ModelGroup>> {
        self.conn
            .query_row(model_group_by_id_sql(), [id], map_model_group)
            .optional()
    }

    pub fn upsert_model_group(&self, group: &ModelGroup) -> Result<()> {
        if group.is_default {
            self.conn
                .execute(clear_other_default_model_groups_sql(), [&group.id])?;
        }
        self.conn.execute(
            "INSERT INTO model_groups (
                id, name, description, status, sort, is_default,
                rate_multiplier_millis, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                status = excluded.status,
                sort = excluded.sort,
                is_default = excluded.is_default,
                rate_multiplier_millis = excluded.rate_multiplier_millis,
                updated_at = excluded.updated_at",
            params![
                &group.id,
                &group.name,
                &group.description,
                &group.status,
                group.sort,
                if group.is_default { 1 } else { 0 },
                group.rate_multiplier_millis,
                group.created_at,
                group.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_model_group(&self, id: &str) -> Result<()> {
        self.conn
            .execute(delete_non_default_model_group_by_id_sql(), [id])?;
        Ok(())
    }

    pub fn list_model_group_models(&self) -> Result<Vec<ModelGroupModel>> {
        let mut stmt = self.conn.prepare(model_group_models_list_sql())?;
        let rows = stmt.query_map([], map_model_group_model)?;
        rows.collect()
    }

    pub fn list_model_group_models_for_group(
        &self,
        group_id: &str,
    ) -> Result<Vec<ModelGroupModel>> {
        let mut stmt = self.conn.prepare(model_group_models_for_group_sql())?;
        let rows = stmt.query_map([group_id], map_model_group_model)?;
        rows.collect()
    }

    pub fn replace_model_group_models(
        &self,
        group_id: &str,
        models: &[ModelGroupModel],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(delete_model_group_models_for_group_sql(), [group_id])?;
        for model in models {
            tx.execute(
                "INSERT INTO model_group_models (
                    group_id, platform_model_slug, enabled, rate_multiplier_millis,
                    billing_model_slug, note, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &model.group_id,
                    &model.platform_model_slug,
                    if model.enabled { 1 } else { 0 },
                    model.rate_multiplier_millis,
                    &model.billing_model_slug,
                    &model.note,
                    model.created_at,
                    model.updated_at
                ],
            )?;
        }
        tx.commit()
    }

    pub fn list_user_model_groups(&self) -> Result<Vec<UserModelGroup>> {
        let mut stmt = self.conn.prepare(user_model_groups_list_sql())?;
        let rows = stmt.query_map([], map_user_model_group)?;
        rows.collect()
    }

    pub fn user_model_group_assignment_count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM user_model_groups", [], |row| {
                row.get(0)
            })
    }

    pub fn list_user_model_groups_for_user(&self, user_id: &str) -> Result<Vec<UserModelGroup>> {
        let mut stmt = self.conn.prepare(user_model_groups_for_user_sql())?;
        let rows = stmt.query_map([user_id], map_user_model_group)?;
        rows.collect()
    }

    pub fn replace_user_model_groups_for_group(
        &self,
        group_id: &str,
        assignments: &[UserModelGroup],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(delete_user_model_groups_for_group_sql(), [group_id])?;
        for assignment in assignments {
            tx.execute(
                "INSERT INTO user_model_groups (
                    user_id, group_id, status, expires_at, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &assignment.user_id,
                    &assignment.group_id,
                    &assignment.status,
                    assignment.expires_at,
                    assignment.created_at,
                    assignment.updated_at
                ],
            )?;
        }
        tx.commit()
    }

    pub fn assign_default_model_group_to_user(&self, user_id: &str) -> Result<()> {
        let Some(group_id) = self.default_model_group_id()? else {
            return Ok(());
        };
        let now = now_ts();
        self.conn.execute(
            "INSERT OR IGNORE INTO user_model_groups (
                user_id, group_id, status, expires_at, created_at, updated_at
             ) VALUES (?1, ?2, 'active', NULL, ?3, ?3)",
            params![user_id, group_id, now],
        )?;
        Ok(())
    }

    pub fn allowed_model_slugs_for_user(&self, user_id: &str, now: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(allowed_model_slugs_for_user_sql())?;
        let rows = stmt.query_map(params![user_id, now], |row| row.get(0))?;
        rows.collect()
    }
    pub fn resolve_model_group_access_for_user(
        &self,
        user_id: &str,
        platform_model_slug: &str,
        now: i64,
    ) -> Result<Option<ModelGroupAccess>> {
        let mut stmt = self.conn.prepare(model_group_access_for_user_sql())?;
        let rows = stmt.query_map(params![user_id, platform_model_slug, now], |row| {
            let group_rate = row.get::<_, i64>(3)?.max(0);
            let model_rate = row.get::<_, Option<i64>>(4)?.unwrap_or(1000).max(0);
            let effective_rate = group_rate.saturating_mul(model_rate) / 1000;
            Ok(ModelGroupAccess {
                group_id: row.get(0)?,
                group_name: row.get(1)?,
                platform_model_slug: row.get(2)?,
                rate_multiplier_millis: effective_rate,
                billing_model_slug: row.get(5)?,
            })
        })?;
        let mut best: Option<ModelGroupAccess> = None;
        for row in rows {
            let access = row?;
            if best.as_ref().is_none_or(|current| {
                access.rate_multiplier_millis < current.rate_multiplier_millis
            }) {
                best = Some(access);
            }
        }
        Ok(best)
    }
}

#[cfg(test)]
#[path = "model_groups_tests.rs"]
mod tests;
