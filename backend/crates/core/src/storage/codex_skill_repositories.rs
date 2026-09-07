use rusqlite::{params, OptionalExtension, Result, Row};

use super::{
    now_ts, CodexSkillRepositoryCatalogSnapshot, CodexSkillRepositoryRecord,
    CodexSkillRepositorySkillRecord, CodexSkillRepositoryUpsert, Storage,
};

fn map_repository(row: &Row<'_>) -> Result<CodexSkillRepositoryRecord> {
    Ok(CodexSkillRepositoryRecord {
        id: row.get(0)?,
        owner: row.get(1)?,
        repository: row.get(2)?,
        ref_name: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        is_builtin: row.get::<_, i64>(5)? != 0,
        revision: row.get(6)?,
        last_scanned_at: row.get(7)?,
        last_error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_skill(row: &Row<'_>) -> Result<CodexSkillRepositorySkillRecord> {
    Ok(CodexSkillRepositorySkillRecord {
        repository_id: row.get(0)?,
        skill_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        path: row.get(4)?,
        source_url: row.get(5)?,
        revision: row.get(6)?,
    })
}

fn repository_columns() -> &'static str {
    "id, owner, repository, ref_name, enabled, is_builtin, revision,
     last_scanned_at, last_error, created_at, updated_at"
}

fn skill_columns() -> &'static str {
    "repository_id, skill_id, name, description, path, source_url, revision"
}

impl Storage {
    pub fn list_codex_skill_repositories(&self) -> Result<Vec<CodexSkillRepositoryRecord>> {
        let sql = format!(
            "SELECT {}
             FROM codex_skill_repositories
             ORDER BY is_builtin DESC, owner COLLATE NOCASE ASC,
                      repository COLLATE NOCASE ASC, ref_name ASC, id ASC",
            repository_columns()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_repository)?;
        rows.collect()
    }

    pub fn get_codex_skill_repository(
        &self,
        id: &str,
    ) -> Result<Option<CodexSkillRepositoryRecord>> {
        let sql = format!(
            "SELECT {}
             FROM codex_skill_repositories
             WHERE id = ?1
             LIMIT 1",
            repository_columns()
        );
        self.conn.query_row(&sql, [id], map_repository).optional()
    }

    pub fn upsert_codex_skill_repository(
        &self,
        input: &CodexSkillRepositoryUpsert,
    ) -> Result<CodexSkillRepositoryRecord> {
        self.conn.execute(
            "INSERT INTO codex_skill_repositories (
                id, owner, repository, ref_name, enabled, is_builtin,
                revision, last_scanned_at, last_error, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                owner = CASE
                    WHEN codex_skill_repositories.is_builtin = 1
                    THEN codex_skill_repositories.owner
                    ELSE excluded.owner
                END,
                repository = CASE
                    WHEN codex_skill_repositories.is_builtin = 1
                    THEN codex_skill_repositories.repository
                    ELSE excluded.repository
                END,
                ref_name = CASE
                    WHEN codex_skill_repositories.is_builtin = 1
                    THEN codex_skill_repositories.ref_name
                    ELSE excluded.ref_name
                END,
                enabled = excluded.enabled,
                is_builtin = MAX(codex_skill_repositories.is_builtin, excluded.is_builtin),
                updated_at = excluded.updated_at",
            params![
                input.id.trim(),
                input.owner.trim(),
                input.repository.trim(),
                input.ref_name.trim(),
                if input.enabled { 1 } else { 0 },
                if input.is_builtin { 1 } else { 0 },
                input.created_at,
                input.updated_at,
            ],
        )?;
        self.get_codex_skill_repository(input.id.trim())?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn set_codex_skill_repository_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE codex_skill_repositories
             SET enabled = ?2, updated_at = ?3
             WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }, now_ts()],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_codex_skill_repository(&self, id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM codex_skill_repositories
             WHERE id = ?1 AND is_builtin = 0",
            [id],
        )?;
        Ok(changed > 0)
    }

    pub fn list_codex_skill_repository_skills(
        &self,
        repository_id: &str,
    ) -> Result<Vec<CodexSkillRepositorySkillRecord>> {
        let sql = format!(
            "SELECT {}
             FROM codex_skill_repository_skills
             WHERE repository_id = ?1
             ORDER BY name COLLATE NOCASE ASC, path COLLATE NOCASE ASC, skill_id ASC",
            skill_columns()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([repository_id], map_skill)?;
        rows.collect()
    }

    pub fn get_codex_skill_repository_skill(
        &self,
        repository_id: &str,
        skill_id: &str,
    ) -> Result<Option<CodexSkillRepositorySkillRecord>> {
        let sql = format!(
            "SELECT {}
             FROM codex_skill_repository_skills
             WHERE repository_id = ?1 AND skill_id = ?2
             LIMIT 1",
            skill_columns()
        );
        self.conn
            .query_row(&sql, [repository_id, skill_id], map_skill)
            .optional()
    }

    pub fn codex_skill_repository_catalog_snapshot(
        &self,
    ) -> Result<CodexSkillRepositoryCatalogSnapshot> {
        let repositories = self.list_codex_skill_repositories()?;
        let sql = format!(
            "SELECT {}
             FROM codex_skill_repository_skills
             ORDER BY repository_id ASC, name COLLATE NOCASE ASC,
                      path COLLATE NOCASE ASC, skill_id ASC",
            skill_columns()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_skill)?;
        Ok(CodexSkillRepositoryCatalogSnapshot {
            repositories,
            skills: rows.collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn replace_codex_skill_repository_snapshot(
        &self,
        repository_id: &str,
        skills: &[CodexSkillRepositorySkillRecord],
        scanned_at: i64,
    ) -> Result<()> {
        let revision = skills
            .first()
            .and_then(|skill| skill.revision.as_deref())
            .map(str::trim)
            .filter(|revision| !revision.is_empty())
            .ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "repository snapshot must contain at least one revision-pinned Skill"
                        .to_string(),
                )
            })?
            .to_string();
        if skills
            .iter()
            .any(|skill| skill.revision.as_deref() != Some(revision.as_str()))
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "repository snapshot Skills must use one revision".to_string(),
            ));
        }
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE codex_skill_repositories
             SET revision = COALESCE(?2, revision),
                 last_scanned_at = ?3,
                 last_error = NULL,
                 updated_at = ?3
             WHERE id = ?1",
            params![repository_id, revision, scanned_at],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        tx.execute(
            "DELETE FROM codex_skill_repository_skills WHERE repository_id = ?1",
            [repository_id],
        )?;
        for skill in skills {
            if skill.repository_id != repository_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "skill repository_id does not match snapshot repository".to_string(),
                ));
            }
            tx.execute(
                "INSERT INTO codex_skill_repository_skills (
                    repository_id, skill_id, name, description, path, source_url, revision
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &skill.repository_id,
                    skill.skill_id.trim(),
                    skill.name.trim(),
                    &skill.description,
                    &skill.path,
                    skill.source_url.trim(),
                    &skill.revision,
                ],
            )?;
        }
        tx.commit()
    }

    pub fn record_codex_skill_repository_error(&self, id: &str, error: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE codex_skill_repositories
             SET last_error = ?2, updated_at = ?3
             WHERE id = ?1",
            params![id, error, now_ts()],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
#[path = "codex_skill_repositories_tests.rs"]
mod tests;
