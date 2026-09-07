use super::{now_ts, ModelSourceMapping, ModelSourceMappingPreference, ModelSourceModel, Storage};
use rusqlite::{params, params_from_iter, types::Value, OptionalExtension, Result, Row};
use std::collections::{BTreeSet, HashMap};

use super::key_id_filters::{text_id_in_clause, SQLITE_IN_CLAUSE_BATCH_SIZE};

fn map_source_model(row: &Row<'_>) -> Result<ModelSourceModel> {
    Ok(ModelSourceModel {
        source_kind: row.get(0)?,
        source_id: row.get(1)?,
        upstream_model: row.get(2)?,
        display_name: row.get(3)?,
        status: row.get(4)?,
        discovery_kind: row.get(5)?,
        last_synced_at: row.get(6)?,
        extra_json: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_source_mapping(row: &Row<'_>) -> Result<ModelSourceMapping> {
    let enabled: i64 = row.get(5)?;
    Ok(ModelSourceMapping {
        id: row.get(0)?,
        platform_model_slug: row.get(1)?,
        source_kind: row.get(2)?,
        source_id: row.get(3)?,
        upstream_model: row.get(4)?,
        enabled: enabled != 0,
        priority: row.get(6)?,
        weight: row.get(7)?,
        billing_model_slug: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn normalize_text(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_source_kinds(source_kinds: &[&str]) -> Vec<String> {
    let mut normalized = source_kinds
        .iter()
        .map(|value| normalize_text(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_slug_values(slugs: &[String]) -> Vec<String> {
    let mut normalized = slugs
        .iter()
        .map(|slug| normalize_text(slug))
        .filter(|slug| !slug.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

impl Storage {
    pub(super) fn ensure_model_source_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_source_models (
                source_kind TEXT NOT NULL,
                source_id TEXT NOT NULL,
                upstream_model TEXT NOT NULL,
                display_name TEXT,
                status TEXT NOT NULL DEFAULT 'available',
                discovery_kind TEXT NOT NULL DEFAULT 'synced',
                last_synced_at INTEGER,
                extra_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (source_kind, source_id, upstream_model)
            );
            CREATE INDEX IF NOT EXISTS idx_model_source_models_upstream_model
                ON model_source_models(upstream_model);
            CREATE INDEX IF NOT EXISTS idx_model_source_models_kind_upstream_status_source
                ON model_source_models(source_kind, upstream_model, status, source_id);
            CREATE INDEX IF NOT EXISTS idx_model_source_models_source_status_upstream
                ON model_source_models(source_kind, source_id, status, upstream_model);
            CREATE TABLE IF NOT EXISTS model_source_mappings (
                id TEXT PRIMARY KEY,
                platform_model_slug TEXT NOT NULL,
                source_kind TEXT NOT NULL,
                source_id TEXT NOT NULL,
                upstream_model TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                priority INTEGER NOT NULL DEFAULT 0,
                weight INTEGER NOT NULL DEFAULT 1,
                billing_model_slug TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(platform_model_slug, source_kind, source_id, upstream_model)
            );
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_source
                ON model_source_mappings(source_kind, source_id, enabled);
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_source_platform
                ON model_source_mappings(source_kind, source_id, platform_model_slug);
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_kind_enabled_platform
                ON model_source_mappings(source_kind, enabled, platform_model_slug);
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_platform_enabled_priority_weight
                ON model_source_mappings(platform_model_slug, enabled, priority DESC, weight DESC, source_kind, source_id, upstream_model);
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_platform_source_enabled_priority
                ON model_source_mappings(platform_model_slug, source_kind, source_id, enabled, priority DESC, weight DESC, upstream_model);
            CREATE INDEX IF NOT EXISTS idx_model_source_mappings_platform_kind_enabled_priority
                ON model_source_mappings(platform_model_slug, source_kind, enabled, priority DESC, weight DESC, source_id, upstream_model);
            CREATE TABLE IF NOT EXISTS model_source_mapping_preferences (
                source_kind     TEXT NOT NULL,
                source_id       TEXT NOT NULL,
                upstream_model  TEXT NOT NULL,
                preference      TEXT NOT NULL CHECK (preference IN ('unlinked', 'disabled')),
                updated_at      INTEGER NOT NULL,
                PRIMARY KEY (source_kind, source_id, upstream_model)
            );
            CREATE INDEX IF NOT EXISTS idx_model_source_mapping_preferences_source
                ON model_source_mapping_preferences(source_kind, source_id);",
        )
    }

    pub fn list_model_source_models(
        &self,
        source_kind: Option<&str>,
        source_id: Option<&str>,
    ) -> Result<Vec<ModelSourceModel>> {
        let normalized_kind = source_kind.map(normalize_text).filter(|v| !v.is_empty());
        let normalized_id = source_id.map(normalize_text).filter(|v| !v.is_empty());
        let sql = model_source_models_list_sql(normalized_kind.is_some(), normalized_id.is_some());

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = match (&normalized_kind, &normalized_id) {
            (Some(kind), Some(id)) => stmt.query_map(params![kind, id], map_source_model)?,
            (Some(kind), None) => stmt.query_map(params![kind], map_source_model)?,
            (None, Some(id)) => stmt.query_map(params![id], map_source_model)?,
            (None, None) => stmt.query_map([], map_source_model)?,
        };
        rows.collect()
    }

    pub fn list_available_model_source_models_for_source(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Vec<ModelSourceModel>> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(available_model_source_models_for_source_sql())?;
        let rows = stmt.query_map(params![source_kind, source_id], map_source_model)?;
        rows.collect()
    }

    pub fn list_available_source_model_ids_by_upstream_model(
        &self,
        source_kind: &str,
        upstream_model: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        let upstream_model = normalize_text(upstream_model);
        if source_kind.is_empty() || upstream_model.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(available_source_model_ids_by_upstream_model_sql())?;
        let rows = stmt.query_map(params![source_kind, upstream_model], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect()
    }

    pub fn list_model_source_model_source_ids_for_kind(
        &self,
        source_kind: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(model_source_model_source_ids_for_kind_sql())?;
        let rows = stmt.query_map(params![source_kind], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn available_source_model_exists(
        &self,
        source_kind: &str,
        source_id: &str,
        upstream_model: &str,
    ) -> Result<bool> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        let upstream_model = normalize_text(upstream_model);
        if source_kind.is_empty() || source_id.is_empty() || upstream_model.is_empty() {
            return Ok(false);
        }
        let found = self
            .conn
            .query_row(
                available_source_model_exists_sql(),
                params![source_kind, source_id, upstream_model],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn upsert_model_source_model(&self, model: &ModelSourceModel) -> Result<()> {
        self.conn.execute(
            "INSERT INTO model_source_models (
                source_kind, source_id, upstream_model, display_name, status, discovery_kind,
                last_synced_at, extra_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(source_kind, source_id, upstream_model) DO UPDATE SET
                display_name = excluded.display_name,
                status = excluded.status,
                discovery_kind = excluded.discovery_kind,
                last_synced_at = excluded.last_synced_at,
                extra_json = excluded.extra_json,
                updated_at = excluded.updated_at",
            params![
                &model.source_kind,
                &model.source_id,
                &model.upstream_model,
                &model.display_name,
                &model.status,
                &model.discovery_kind,
                model.last_synced_at,
                &model.extra_json,
                model.created_at,
                model.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_discovered_model_source_models(
        &self,
        source_kind: &str,
        source_id: &str,
        upstream_models: &[String],
        discovery_kind: &str,
    ) -> Result<Vec<ModelSourceModel>> {
        let now = now_ts();
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        let discovery_kind = normalize_text(discovery_kind);
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for upstream_model in upstream_models {
            let upstream_model = normalize_text(upstream_model);
            if upstream_model.is_empty() || !seen.insert(upstream_model.clone()) {
                continue;
            }
            let record = ModelSourceModel {
                source_kind: source_kind.clone(),
                source_id: source_id.clone(),
                display_name: Some(upstream_model.clone()),
                upstream_model,
                status: "available".to_string(),
                discovery_kind: discovery_kind.clone(),
                last_synced_at: Some(now),
                extra_json: "{}".to_string(),
                created_at: now,
                updated_at: now,
            };
            self.upsert_model_source_model(&record)?;
            out.push(record);
        }

        let mut stmt = self.conn.prepare(
            "SELECT upstream_model
             FROM model_source_models
             WHERE source_kind = ?1
               AND source_id = ?2
               AND discovery_kind = ?3",
        )?;
        let existing_rows = stmt
            .query_map(params![&source_kind, &source_id, &discovery_kind], |row| {
                row.get::<_, String>(0)
            })?;
        let existing_upstream_models: std::collections::BTreeSet<String> = existing_rows
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .collect();
        let stale_upstream_models = existing_upstream_models
            .difference(&seen)
            .cloned()
            .collect::<Vec<_>>();
        for upstream_model in stale_upstream_models {
            self.conn.execute(
                delete_model_source_mappings_for_source_upstream_sql(),
                params![&source_kind, &source_id, &upstream_model],
            )?;
            self.conn.execute(
                delete_model_source_model_for_source_discovery_upstream_sql(),
                params![&source_kind, &source_id, &upstream_model, &discovery_kind],
            )?;
            self.conn.execute(
                delete_model_source_mapping_preference_sql(),
                params![&source_kind, &source_id, &upstream_model],
            )?;
        }
        Ok(out)
    }

    pub fn list_model_source_mappings(
        &self,
        platform_model_slug: Option<&str>,
    ) -> Result<Vec<ModelSourceMapping>> {
        let normalized_slug = platform_model_slug
            .map(normalize_text)
            .filter(|value| !value.is_empty());
        let mut stmt = self
            .conn
            .prepare(model_source_mappings_list_sql(normalized_slug.is_some()))?;
        let rows = if let Some(slug) = normalized_slug {
            stmt.query_map(params![slug], map_source_mapping)?
        } else {
            stmt.query_map([], map_source_mapping)?
        };
        rows.collect()
    }

    pub fn list_enabled_model_source_mappings_for_platform(
        &self,
        platform_model_slug: &str,
    ) -> Result<Vec<ModelSourceMapping>> {
        let slug = normalize_text(platform_model_slug);
        if slug.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(enabled_model_source_mappings_for_platform_sql())?;
        let rows = stmt.query_map(params![slug], map_source_mapping)?;
        rows.collect()
    }

    pub fn list_enabled_model_source_mappings_for_platform_and_kind(
        &self,
        platform_model_slug: &str,
        source_kind: &str,
    ) -> Result<Vec<ModelSourceMapping>> {
        let slug = normalize_text(platform_model_slug);
        let source_kind = normalize_text(source_kind);
        if slug.is_empty() || source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(enabled_model_source_mappings_for_platform_and_kind_sql())?;
        let rows = stmt.query_map(params![slug, source_kind], map_source_mapping)?;
        rows.collect()
    }

    pub fn list_enabled_model_source_mapping_source_ids_for_platform_and_kind(
        &self,
        platform_model_slug: &str,
        source_kind: &str,
    ) -> Result<Vec<String>> {
        let slug = normalize_text(platform_model_slug);
        let source_kind = normalize_text(source_kind);
        if slug.is_empty() || source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(enabled_model_source_mapping_source_ids_for_platform_and_kind_sql())?;
        let rows = stmt.query_map(params![slug, source_kind], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_model_source_mapping_source_ids_for_kind(
        &self,
        source_kind: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(model_source_mapping_source_ids_for_kind_sql())?;
        let rows = stmt.query_map(params![source_kind], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_model_route_source_ids_for_kind(&self, source_kind: &str) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(model_route_source_ids_for_kind_sql())?;
        let rows = stmt.query_map(params![source_kind], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_model_source_mapping_platform_slugs_for_source(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(model_source_mapping_platform_slugs_for_source_sql())?;
        let rows = stmt.query_map(params![source_kind, source_id], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect()
    }

    pub fn list_enabled_model_source_mapping_platform_slugs_for_kind(
        &self,
        source_kind: &str,
    ) -> Result<Vec<String>> {
        let source_kind = normalize_text(source_kind);
        if source_kind.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(enabled_model_source_mapping_platform_slugs_for_kind_sql())?;
        let rows = stmt.query_map(params![source_kind], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_enabled_model_source_mapping_platform_slugs(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(enabled_model_source_mapping_platform_slugs_sql())?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_enabled_model_source_mapping_platform_slugs_for_platforms(
        &self,
        platform_slugs: &[String],
    ) -> Result<Vec<String>> {
        let slugs = normalize_slug_values(platform_slugs);
        if slugs.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = BTreeSet::new();
        for chunk in slugs.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            let Some((condition, params)) = text_id_in_clause("platform_model_slug", chunk) else {
                continue;
            };
            let sql =
                enabled_model_source_mapping_platform_slugs_for_platforms_chunk_sql(&condition);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params), |row| row.get::<_, String>(0))?;
            for row in rows {
                found.insert(row?);
            }
        }
        Ok(found.into_iter().collect())
    }

    pub fn list_model_source_model_upstream_models_for_upstream_models(
        &self,
        upstream_models: &[String],
    ) -> Result<Vec<String>> {
        let upstream_models = normalize_slug_values(upstream_models);
        if upstream_models.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = BTreeSet::new();
        for chunk in upstream_models.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            let Some((condition, params)) = text_id_in_clause("upstream_model", chunk) else {
                continue;
            };
            let sql = model_source_model_upstream_models_for_upstream_models_chunk_sql(&condition);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params), |row| row.get::<_, String>(0))?;
            for row in rows {
                found.insert(row?);
            }
        }
        Ok(found.into_iter().collect())
    }

    pub fn has_enabled_model_source_mapping_for_platform_and_kind(
        &self,
        platform_model_slug: &str,
        source_kind: &str,
    ) -> Result<bool> {
        let slug = normalize_text(platform_model_slug);
        let source_kind = normalize_text(source_kind);
        if slug.is_empty() || source_kind.is_empty() {
            return Ok(false);
        }
        let found = self
            .conn
            .query_row(
                enabled_model_source_mapping_exists_for_platform_and_kind_sql(),
                params![slug, source_kind],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn has_enabled_model_source_mapping_for_platform(
        &self,
        platform_model_slug: &str,
    ) -> Result<bool> {
        let slug = normalize_text(platform_model_slug);
        if slug.is_empty() {
            return Ok(false);
        }
        let found = self
            .conn
            .query_row(
                enabled_model_source_mapping_exists_for_platform_sql(),
                params![slug],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn has_enabled_model_source_mapping_for_platform_matching_kinds(
        &self,
        platform_model_slug: &str,
        source_kinds: &[&str],
    ) -> Result<bool> {
        let slug = normalize_text(platform_model_slug);
        let source_kinds = normalize_source_kinds(source_kinds);
        if slug.is_empty() || source_kinds.is_empty() {
            return Ok(false);
        }
        let Some((source_kind_condition, source_kind_params)) =
            text_id_in_clause("source_kind", &source_kinds)
        else {
            return Ok(false);
        };
        let sql = format!(
            "SELECT 1
             FROM model_source_mappings
             WHERE platform_model_slug = ?1
               AND enabled = 1
               AND {source_kind_condition}
             LIMIT 1"
        );
        let mut params = Vec::with_capacity(source_kinds.len() + 1);
        params.push(Value::Text(slug));
        params.extend(source_kind_params);
        let found = self
            .conn
            .query_row(&sql, params_from_iter(params), |_| Ok(()))
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn has_enabled_model_source_mapping_for_platform_outside_kinds(
        &self,
        platform_model_slug: &str,
        source_kinds: &[&str],
    ) -> Result<bool> {
        let slug = normalize_text(platform_model_slug);
        let source_kinds = normalize_source_kinds(source_kinds);
        if slug.is_empty() {
            return Ok(false);
        }
        if source_kinds.is_empty() {
            let found = self
                .conn
                .query_row(
                    enabled_model_source_mapping_exists_for_platform_sql(),
                    params![slug],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            return Ok(found);
        }
        let Some((source_kind_condition, source_kind_params)) =
            text_id_in_clause("source_kind", &source_kinds)
        else {
            return Ok(false);
        };
        let sql = format!(
            "SELECT 1
             FROM model_source_mappings
             WHERE platform_model_slug = ?1
               AND enabled = 1
               AND NOT ({source_kind_condition})
             LIMIT 1"
        );
        let mut params = Vec::with_capacity(source_kinds.len() + 1);
        params.push(Value::Text(slug));
        params.extend(source_kind_params);
        let found = self
            .conn
            .query_row(&sql, params_from_iter(params), |_| Ok(()))
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn find_enabled_model_source_mapping(
        &self,
        platform_model_slug: &str,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Option<ModelSourceMapping>> {
        self.conn
            .query_row(
                enabled_model_source_mapping_for_platform_source_sql(),
                params![
                    normalize_text(platform_model_slug),
                    normalize_text(source_kind),
                    normalize_text(source_id),
                ],
                map_source_mapping,
            )
            .optional()
    }

    pub fn list_enabled_model_source_mappings_for_sources(
        &self,
        platform_model_slug: &str,
        source_kind: &str,
        source_ids: &[String],
    ) -> Result<HashMap<String, ModelSourceMapping>> {
        let slug = normalize_text(platform_model_slug);
        let source_kind = normalize_text(source_kind);
        let source_ids = normalize_slug_values(source_ids);
        if slug.is_empty() || source_kind.is_empty() || source_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut out = HashMap::new();
        for chunk in source_ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            for mapping in list_enabled_model_source_mappings_for_sources_chunk(
                self,
                slug.as_str(),
                source_kind.as_str(),
                chunk,
            )? {
                out.insert(mapping.source_id.clone(), mapping);
            }
        }
        Ok(out)
    }

    pub fn upsert_model_source_mapping(&self, mapping: &ModelSourceMapping) -> Result<()> {
        self.conn.execute(
            "INSERT INTO model_source_mappings (
                id, platform_model_slug, source_kind, source_id, upstream_model,
                enabled, priority, weight, billing_model_slug, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(platform_model_slug, source_kind, source_id, upstream_model) DO UPDATE SET
                enabled = excluded.enabled,
                priority = excluded.priority,
                weight = excluded.weight,
                billing_model_slug = excluded.billing_model_slug,
                updated_at = excluded.updated_at",
            params![
                &mapping.id,
                &mapping.platform_model_slug,
                &mapping.source_kind,
                &mapping.source_id,
                &mapping.upstream_model,
                if mapping.enabled { 1 } else { 0 },
                mapping.priority,
                mapping.weight,
                &mapping.billing_model_slug,
                mapping.created_at,
                mapping.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_model_source_mapping_with_unlink_preference(
        &self,
        id: &str,
        source_kind: &str,
        source_id: &str,
        upstream_model: &str,
    ) -> Result<()> {
        let id = normalize_text(id);
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        let upstream_model = normalize_text(upstream_model);
        if source_kind.is_empty() || source_id.is_empty() || upstream_model.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO model_source_mapping_preferences
             (source_kind, source_id, upstream_model, preference, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_kind, source_id, upstream_model) DO UPDATE SET
                 preference = excluded.preference,
                 updated_at = excluded.updated_at",
            params![
                &source_kind,
                &source_id,
                &upstream_model,
                "unlinked",
                now_ts()
            ],
        )?;
        tx.execute(delete_model_source_mapping_by_id_sql(), params![&id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_model_source_mapping(&self, id: &str) -> Result<()> {
        self.conn.execute(
            delete_model_source_mapping_by_id_sql(),
            params![normalize_text(id)],
        )?;
        Ok(())
    }

    pub fn delete_model_source_routes_for_source(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<()> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            delete_model_source_mappings_for_source_sql(),
            params![&source_kind, &source_id],
        )?;
        self.conn.execute(
            delete_model_source_models_for_source_sql(),
            params![&source_kind, &source_id],
        )?;
        Ok(())
    }

    pub fn upsert_model_source_mapping_preference(
        &self,
        source_kind: &str,
        source_id: &str,
        upstream_model: &str,
        preference: &str,
    ) -> Result<()> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        let upstream_model = normalize_text(upstream_model);
        if source_kind.is_empty() || source_id.is_empty() || upstream_model.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO model_source_mapping_preferences
             (source_kind, source_id, upstream_model, preference, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_kind, source_id, upstream_model) DO UPDATE SET
                 preference = excluded.preference,
                 updated_at = excluded.updated_at",
            params![
                &source_kind,
                &source_id,
                &upstream_model,
                normalize_text(preference),
                now_ts(),
            ],
        )?;
        Ok(())
    }

    pub fn delete_model_source_mapping_preference(
        &self,
        source_kind: &str,
        source_id: &str,
        upstream_model: &str,
    ) -> Result<()> {
        self.conn.execute(
            delete_model_source_mapping_preference_sql(),
            params![
                normalize_text(source_kind),
                normalize_text(source_id),
                normalize_text(upstream_model),
            ],
        )?;
        Ok(())
    }

    pub fn delete_model_source_mapping_preferences_for_source(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<()> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            delete_model_source_mapping_preferences_for_source_sql(),
            params![&source_kind, &source_id],
        )?;
        Ok(())
    }

    pub fn list_model_source_mapping_preferences(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Vec<ModelSourceMappingPreference>> {
        let source_kind = normalize_text(source_kind);
        let source_id = normalize_text(source_id);
        if source_kind.is_empty() || source_id.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(model_source_mapping_preferences_for_source_sql())?;
        let rows = stmt.query_map(params![&source_kind, &source_id], map_preference)?;
        rows.collect()
    }

    pub fn delete_model_source_routes_for_platform_model(
        &self,
        platform_model_slug: &str,
    ) -> Result<()> {
        let slug = normalize_text(platform_model_slug);
        if slug.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            delete_model_source_mappings_for_platform_model_sql(),
            params![&slug],
        )?;
        Ok(())
    }
}

fn list_enabled_model_source_mappings_for_sources_chunk(
    storage: &Storage,
    platform_model_slug: &str,
    source_kind: &str,
    source_ids: &[String],
) -> Result<Vec<ModelSourceMapping>> {
    if source_ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some((source_condition, source_params)) = text_id_in_clause("source_id", source_ids) else {
        return Ok(Vec::new());
    };
    let sql = enabled_model_source_mappings_for_sources_chunk_sql(&source_condition);
    let mut params = Vec::with_capacity(source_ids.len() + 2);
    params.push(Value::Text(platform_model_slug.to_string()));
    params.push(Value::Text(source_kind.to_string()));
    params.extend(source_params);
    let mut stmt = storage.conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), map_source_mapping)?;
    rows.collect()
}

fn model_source_models_list_sql(has_source_kind: bool, has_source_id: bool) -> String {
    let mut sql = "SELECT source_kind, source_id, upstream_model, display_name, status,
                        discovery_kind, last_synced_at, extra_json, created_at, updated_at
                   FROM model_source_models"
        .to_string();
    match (has_source_kind, has_source_id) {
        (true, true) => sql.push_str(" WHERE source_kind = ?1 AND source_id = ?2"),
        (true, false) => sql.push_str(" WHERE source_kind = ?1"),
        (false, true) => sql.push_str(" WHERE source_id = ?1"),
        (false, false) => {}
    }
    sql.push_str(" ORDER BY source_kind ASC, source_id ASC, upstream_model ASC");
    sql
}

fn enabled_model_source_mappings_for_sources_chunk_sql(source_condition: &str) -> String {
    format!(
        "WITH ranked AS (
            SELECT
                id,
                platform_model_slug,
                source_kind,
                source_id,
                upstream_model,
                enabled,
                priority,
                weight,
                billing_model_slug,
                created_at,
                updated_at,
                ROW_NUMBER() OVER (
                    PARTITION BY source_id
                    ORDER BY priority DESC, weight DESC, upstream_model ASC
                ) AS rn
            FROM model_source_mappings
            WHERE platform_model_slug = ?1
              AND source_kind = ?2
              AND enabled = 1
              AND {source_condition}
        )
        SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
               enabled, priority, weight, billing_model_slug, created_at, updated_at
        FROM ranked
        WHERE rn = 1"
    )
}

fn available_model_source_models_for_source_sql() -> &'static str {
    "SELECT source_kind, source_id, upstream_model, display_name, status,
            discovery_kind, last_synced_at, extra_json, created_at, updated_at
     FROM model_source_models
     WHERE source_kind = ?1
       AND source_id = ?2
       AND status = 'available'
       AND TRIM(upstream_model) <> ''
     ORDER BY upstream_model ASC"
}

fn available_source_model_ids_by_upstream_model_sql() -> &'static str {
    "SELECT DISTINCT source_id
     FROM model_source_models
     WHERE source_kind = ?1
       AND upstream_model = ?2
       AND status = 'available'
     ORDER BY source_id ASC"
}

fn model_source_model_source_ids_for_kind_sql() -> &'static str {
    "SELECT DISTINCT source_id
     FROM model_source_models
     WHERE source_kind = ?1
       AND TRIM(source_id) <> ''
     ORDER BY source_id ASC"
}

fn available_source_model_exists_sql() -> &'static str {
    "SELECT 1
     FROM model_source_models
     WHERE source_kind = ?1
       AND source_id = ?2
       AND upstream_model = ?3
       AND status = 'available'
     LIMIT 1"
}

fn model_source_mappings_list_sql(has_platform_slug: bool) -> &'static str {
    if has_platform_slug {
        "SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
                enabled, priority, weight, billing_model_slug, created_at, updated_at
         FROM model_source_mappings
         WHERE platform_model_slug = ?1
         ORDER BY enabled DESC, priority DESC, source_kind ASC, source_id ASC, upstream_model ASC"
    } else {
        "SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
                enabled, priority, weight, billing_model_slug, created_at, updated_at
         FROM model_source_mappings
         ORDER BY platform_model_slug ASC, enabled DESC, priority DESC,
                  source_kind ASC, source_id ASC, upstream_model ASC"
    }
}

fn enabled_model_source_mappings_for_platform_sql() -> &'static str {
    "SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
            enabled, priority, weight, billing_model_slug, created_at, updated_at
     FROM model_source_mappings
     WHERE platform_model_slug = ?1 AND enabled = 1
     ORDER BY priority DESC, weight DESC, source_kind ASC, source_id ASC, upstream_model ASC"
}

fn enabled_model_source_mappings_for_platform_and_kind_sql() -> &'static str {
    "SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
            enabled, priority, weight, billing_model_slug, created_at, updated_at
     FROM model_source_mappings
     WHERE platform_model_slug = ?1
       AND source_kind = ?2
       AND enabled = 1
     ORDER BY priority DESC, weight DESC, source_id ASC, upstream_model ASC"
}

fn enabled_model_source_mapping_source_ids_for_platform_and_kind_sql() -> &'static str {
    "SELECT DISTINCT source_id
     FROM model_source_mappings
     WHERE platform_model_slug = ?1
       AND source_kind = ?2
       AND enabled = 1
     ORDER BY source_id ASC"
}

fn model_source_mapping_source_ids_for_kind_sql() -> &'static str {
    "SELECT DISTINCT source_id
     FROM model_source_mappings
     WHERE source_kind = ?1
     ORDER BY source_id ASC"
}

fn model_route_source_ids_for_kind_sql() -> &'static str {
    "SELECT source_id
     FROM (
        SELECT source_id
        FROM model_source_models
        WHERE source_kind = ?1
          AND TRIM(source_id) <> ''
        UNION
        SELECT source_id
        FROM model_source_mappings
        WHERE source_kind = ?1
          AND TRIM(source_id) <> ''
     )
     ORDER BY source_id ASC"
}

fn model_source_mapping_platform_slugs_for_source_sql() -> &'static str {
    "SELECT DISTINCT platform_model_slug
     FROM model_source_mappings
     WHERE source_kind = ?1
       AND source_id = ?2
     ORDER BY platform_model_slug ASC"
}

fn enabled_model_source_mapping_platform_slugs_for_kind_sql() -> &'static str {
    "SELECT DISTINCT platform_model_slug
     FROM model_source_mappings
     WHERE source_kind = ?1
       AND enabled = 1
     ORDER BY platform_model_slug ASC"
}

fn enabled_model_source_mapping_platform_slugs_sql() -> &'static str {
    "SELECT DISTINCT platform_model_slug
     FROM model_source_mappings
     WHERE enabled = 1
     ORDER BY platform_model_slug ASC"
}

fn model_source_mapping_preferences_for_source_sql() -> &'static str {
    "SELECT source_kind, source_id, upstream_model, preference, updated_at
     FROM model_source_mapping_preferences
     WHERE source_kind = ?1 AND source_id = ?2"
}

fn delete_model_source_mapping_by_id_sql() -> &'static str {
    "DELETE FROM model_source_mappings WHERE id = ?1"
}

fn delete_model_source_mappings_for_source_upstream_sql() -> &'static str {
    "DELETE FROM model_source_mappings
     WHERE source_kind = ?1 AND source_id = ?2 AND upstream_model = ?3"
}

fn delete_model_source_model_for_source_discovery_upstream_sql() -> &'static str {
    "DELETE FROM model_source_models
     WHERE source_kind = ?1
       AND source_id = ?2
       AND upstream_model = ?3
       AND discovery_kind = ?4"
}

pub(super) fn delete_model_source_mappings_for_source_sql() -> &'static str {
    "DELETE FROM model_source_mappings WHERE source_kind = ?1 AND source_id = ?2"
}

pub(super) fn delete_model_source_models_for_source_sql() -> &'static str {
    "DELETE FROM model_source_models WHERE source_kind = ?1 AND source_id = ?2"
}

fn delete_model_source_mapping_preference_sql() -> &'static str {
    "DELETE FROM model_source_mapping_preferences
     WHERE source_kind = ?1 AND source_id = ?2 AND upstream_model = ?3"
}

pub(super) fn delete_model_source_mapping_preferences_for_source_sql() -> &'static str {
    "DELETE FROM model_source_mapping_preferences
     WHERE source_kind = ?1 AND source_id = ?2"
}

fn delete_model_source_mappings_for_platform_model_sql() -> &'static str {
    "DELETE FROM model_source_mappings
     WHERE platform_model_slug = ?1"
}

fn enabled_model_source_mapping_exists_for_platform_sql() -> &'static str {
    "SELECT 1
     FROM model_source_mappings
     WHERE platform_model_slug = ?1
       AND enabled = 1
     LIMIT 1"
}

fn enabled_model_source_mapping_exists_for_platform_and_kind_sql() -> &'static str {
    "SELECT 1
     FROM model_source_mappings
     WHERE platform_model_slug = ?1
       AND source_kind = ?2
       AND enabled = 1
     LIMIT 1"
}

fn enabled_model_source_mapping_for_platform_source_sql() -> &'static str {
    "SELECT id, platform_model_slug, source_kind, source_id, upstream_model,
            enabled, priority, weight, billing_model_slug, created_at, updated_at
     FROM model_source_mappings
     WHERE platform_model_slug = ?1
       AND source_kind = ?2
       AND source_id = ?3
       AND enabled = 1
     ORDER BY priority DESC, weight DESC, upstream_model ASC
     LIMIT 1"
}

fn enabled_model_source_mapping_platform_slugs_for_platforms_chunk_sql(
    platform_condition: &str,
) -> String {
    format!(
        "SELECT DISTINCT platform_model_slug
         FROM model_source_mappings
         WHERE enabled = 1
           AND {platform_condition}"
    )
}

fn model_source_model_upstream_models_for_upstream_models_chunk_sql(
    upstream_condition: &str,
) -> String {
    format!(
        "SELECT DISTINCT upstream_model
         FROM model_source_models
         WHERE {upstream_condition}"
    )
}

fn map_preference(row: &Row<'_>) -> Result<ModelSourceMappingPreference> {
    Ok(ModelSourceMappingPreference {
        source_kind: row.get(0)?,
        source_id: row.get(1)?,
        upstream_model: row.get(2)?,
        preference: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

#[cfg(test)]
#[path = "model_sources_tests.rs"]
mod tests;
