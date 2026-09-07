use std::collections::HashMap;

use super::{
    now_ts, PluginInstall, PluginInstallListSummary, PluginRunLog, PluginRunLogListSummary,
    PluginRuntimeInstall, PluginTask, PluginTaskCount, PluginTaskExecutionRow,
    PluginTaskListSummary, PluginTaskScheduleRepairRow, Storage,
};
use crate::storage::key_id_filters::{
    normalize_text_ids, text_id_in_clause, SQLITE_IN_CLAUSE_BATCH_SIZE,
};
use rusqlite::{params, params_from_iter, types::Value, Result, Row};

fn plugin_install_list_order_sql(select_columns: &str) -> String {
    format!(
        "SELECT {select_columns}
         FROM plugin_installs
         ORDER BY updated_at DESC, installed_at DESC, plugin_id ASC"
    )
}

fn plugin_install_summary_list_sql() -> String {
    plugin_install_list_order_sql(
        "
                plugin_id,
                source_url,
                name,
                version,
                description,
                author,
                homepage_url,
                script_url,
                permissions_json,
                status,
                installed_at,
                updated_at,
                last_run_at,
                last_error,
                COALESCE(
                    json_extract(manifest_json, '$.manifestVersion'),
                    json_extract(manifest_json, '$.manifest_version')
                ),
                json_extract(manifest_json, '$.category'),
                COALESCE(
                    json_extract(manifest_json, '$.runtimeKind'),
                    json_extract(manifest_json, '$.runtime_kind')
                ),
                json_extract(manifest_json, '$.tags')
            ",
    )
}

fn plugin_install_by_id_sql(select_columns: &str) -> String {
    format!(
        "SELECT {select_columns}
         FROM plugin_installs
         WHERE plugin_id = ?1
         LIMIT 1"
    )
}

fn plugin_task_list_sql(select_columns: &str, plugin_filter: bool) -> String {
    let mut sql = format!(
        "SELECT {select_columns}
         FROM plugin_tasks"
    );
    if plugin_filter {
        sql.push_str("\n         WHERE plugin_id = ?1");
    }
    sql.push_str("\n         ORDER BY next_run_at ASC, created_at ASC");
    sql
}

fn plugin_task_summary_list_sql(plugin_filter: bool) -> String {
    let mut sql = "SELECT
            t.id,
            t.plugin_id,
            COALESCE(p.name, t.plugin_id),
            t.name,
            t.description,
            t.entrypoint,
            t.schedule_kind,
            t.interval_seconds,
            t.enabled,
            t.next_run_at,
            t.last_run_at,
            t.last_status,
            t.last_error
         FROM plugin_tasks t
         LEFT JOIN plugin_installs p ON p.plugin_id = t.plugin_id"
        .to_string();
    if plugin_filter {
        sql.push_str("\n         WHERE t.plugin_id = ?1");
    }
    sql.push_str("\n         ORDER BY t.next_run_at ASC, t.created_at ASC");
    sql
}

fn due_plugin_tasks_sql(select_columns: &str) -> String {
    format!(
        "SELECT {select_columns}
         FROM plugin_tasks t
         INNER JOIN plugin_installs p ON p.plugin_id = t.plugin_id
         WHERE t.enabled = 1
           AND p.status = 'enabled'
           AND t.schedule_kind <> 'manual'
           AND (t.next_run_at IS NULL OR t.next_run_at <= ?1)
         ORDER BY IFNULL(t.next_run_at, t.created_at) ASC, t.created_at ASC
         LIMIT ?2"
    )
}

fn next_enabled_plugin_task_run_at_sql() -> &'static str {
    "SELECT MIN(t.next_run_at)
     FROM plugin_tasks t
     INNER JOIN plugin_installs p ON p.plugin_id = t.plugin_id
     WHERE t.enabled = 1
       AND p.status = 'enabled'
       AND t.schedule_kind <> 'manual'
       AND t.next_run_at IS NOT NULL"
}

fn plugin_install_names_for_plugins_chunk_sql(plugin_condition: &str) -> String {
    format!(
        "SELECT plugin_id, name
         FROM plugin_installs
         WHERE {plugin_condition}"
    )
}

fn delete_plugin_tasks_for_plugin_sql() -> &'static str {
    "DELETE FROM plugin_tasks WHERE plugin_id = ?1"
}

fn delete_plugin_install_by_id_sql() -> &'static str {
    "DELETE FROM plugin_installs WHERE plugin_id = ?1"
}

fn plugin_task_names_for_tasks_chunk_sql(task_condition: &str) -> String {
    format!(
        "SELECT id, name
         FROM plugin_tasks
         WHERE {task_condition}"
    )
}

fn plugin_task_counts_by_plugin_sql() -> &'static str {
    "SELECT plugin_id, COUNT(*) AS task_count, SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END) AS enabled_task_count
     FROM plugin_tasks
     GROUP BY plugin_id"
}

fn plugin_task_by_id_sql(select_columns: &str) -> String {
    format!(
        "SELECT {select_columns}
         FROM plugin_tasks
         WHERE id = ?1
         LIMIT 1"
    )
}

fn repair_plugin_task_schedules_sql(plugin_filter: bool) -> &'static str {
    if plugin_filter {
        "UPDATE plugin_tasks
         SET next_run_at = COALESCE(last_run_at + interval_seconds, ?1),
             updated_at = ?2
         WHERE plugin_id = ?3
           AND enabled = 1
           AND schedule_kind <> 'manual'
           AND next_run_at IS NULL
           AND interval_seconds IS NOT NULL
           AND interval_seconds > 0"
    } else {
        "UPDATE plugin_tasks
         SET next_run_at = COALESCE(last_run_at + interval_seconds, ?1),
             updated_at = ?2
         WHERE enabled = 1
           AND schedule_kind <> 'manual'
           AND next_run_at IS NULL
           AND interval_seconds IS NOT NULL
           AND interval_seconds > 0"
    }
}

fn update_plugin_task_enabled_sql() -> &'static str {
    "UPDATE plugin_tasks
     SET enabled = ?1, updated_at = ?2
     WHERE id = ?3"
}

fn update_plugin_task_definition_sql() -> &'static str {
    "UPDATE plugin_tasks
     SET name = ?,
         description = ?,
         entrypoint = ?,
         schedule_kind = ?,
         interval_seconds = ?,
         enabled = ?,
         next_run_at = ?,
         task_json = ?,
         updated_at = ?
     WHERE id = ?"
}

fn update_plugin_task_schedule_sql() -> &'static str {
    "UPDATE plugin_tasks
     SET next_run_at = ?1, last_run_at = ?2, last_status = ?3, last_error = ?4, updated_at = ?5
     WHERE id = ?6"
}

fn plugin_run_log_list_sql(plugin_filter: bool, task_filter: bool) -> String {
    let mut sql = "SELECT id, plugin_id, task_id, run_type, status, started_at, finished_at, duration_ms, output_json, error
             FROM plugin_run_logs"
        .to_string();
    let mut where_clauses = Vec::new();
    if plugin_filter {
        where_clauses.push("plugin_id = ?");
    }
    if task_filter {
        where_clauses.push("task_id = ?");
    }
    if !where_clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY started_at DESC, id DESC LIMIT ?");
    sql
}

fn plugin_run_log_summary_list_sql(plugin_filter: bool, task_filter: bool) -> String {
    let mut sql = "SELECT
                l.id,
                l.plugin_id,
                p.name,
                l.task_id,
                t.name,
                l.run_type,
                l.status,
                l.started_at,
                l.finished_at,
                l.duration_ms,
                l.output_json,
                l.error
             FROM plugin_run_logs l
             LEFT JOIN plugin_installs p ON p.plugin_id = l.plugin_id
             LEFT JOIN plugin_tasks t ON t.id = l.task_id"
        .to_string();
    let mut where_clauses = Vec::new();
    if plugin_filter {
        where_clauses.push("l.plugin_id = ?");
    }
    if task_filter {
        where_clauses.push("l.task_id = ?");
    }
    if !where_clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY l.started_at DESC, l.id DESC LIMIT ?");
    sql
}

impl Storage {
    /// 函数 `upsert_plugin_install`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin: 参数 plugin
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn upsert_plugin_install(&self, plugin: &PluginInstall) -> Result<()> {
        self.conn.execute(
            "INSERT INTO plugin_installs (
                plugin_id, source_url, name, version, description, author, homepage_url, script_url,
                script_body, permissions_json, manifest_json, status, installed_at, updated_at,
                last_run_at, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(plugin_id) DO UPDATE SET
                source_url = excluded.source_url,
                name = excluded.name,
                version = excluded.version,
                description = excluded.description,
                author = excluded.author,
                homepage_url = excluded.homepage_url,
                script_url = excluded.script_url,
                script_body = excluded.script_body,
                permissions_json = excluded.permissions_json,
                manifest_json = excluded.manifest_json,
                status = excluded.status,
                updated_at = excluded.updated_at,
                last_run_at = excluded.last_run_at,
                last_error = excluded.last_error",
            params![
                &plugin.plugin_id,
                &plugin.source_url,
                &plugin.name,
                &plugin.version,
                &plugin.description,
                &plugin.author,
                &plugin.homepage_url,
                &plugin.script_url,
                &plugin.script_body,
                &plugin.permissions_json,
                &plugin.manifest_json,
                &plugin.status,
                plugin.installed_at,
                plugin.updated_at,
                plugin.last_run_at,
                &plugin.last_error,
            ],
        )?;
        Ok(())
    }

    /// 函数 `replace_plugin_install`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin: 参数 plugin
    /// - tasks: 参数 tasks
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn replace_plugin_install(
        &self,
        plugin: &PluginInstall,
        tasks: &[PluginTask],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO plugin_installs (
                plugin_id, source_url, name, version, description, author, homepage_url, script_url,
                script_body, permissions_json, manifest_json, status, installed_at, updated_at,
                last_run_at, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(plugin_id) DO UPDATE SET
                source_url = excluded.source_url,
                name = excluded.name,
                version = excluded.version,
                description = excluded.description,
                author = excluded.author,
                homepage_url = excluded.homepage_url,
                script_url = excluded.script_url,
                script_body = excluded.script_body,
                permissions_json = excluded.permissions_json,
                manifest_json = excluded.manifest_json,
                status = excluded.status,
                updated_at = excluded.updated_at,
                last_run_at = excluded.last_run_at,
                last_error = excluded.last_error",
            params![
                &plugin.plugin_id,
                &plugin.source_url,
                &plugin.name,
                &plugin.version,
                &plugin.description,
                &plugin.author,
                &plugin.homepage_url,
                &plugin.script_url,
                &plugin.script_body,
                &plugin.permissions_json,
                &plugin.manifest_json,
                &plugin.status,
                plugin.installed_at,
                plugin.updated_at,
                plugin.last_run_at,
                &plugin.last_error,
            ],
        )?;
        tx.execute(delete_plugin_tasks_for_plugin_sql(), [&plugin.plugin_id])?;
        for task in tasks {
            tx.execute(
                "INSERT INTO plugin_tasks (
                    id, plugin_id, name, description, entrypoint, schedule_kind, interval_seconds,
                    enabled, next_run_at, last_run_at, last_status, last_error, task_json, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    &task.id,
                    &task.plugin_id,
                    &task.name,
                    &task.description,
                    &task.entrypoint,
                    &task.schedule_kind,
                    &task.interval_seconds,
                    if task.enabled { 1_i64 } else { 0_i64 },
                    &task.next_run_at,
                    &task.last_run_at,
                    &task.last_status,
                    &task.last_error,
                    &task.task_json,
                    task.created_at,
                    task.updated_at,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 函数 `list_plugin_installs`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_plugin_installs(&self) -> Result<Vec<PluginInstall>> {
        let sql = plugin_install_list_order_sql(
            "
                plugin_id, source_url, name, version, description, author, homepage_url, script_url,
                script_body, permissions_json, manifest_json, status, installed_at, updated_at,
                last_run_at, last_error
            ",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_plugin_install_row(row)?);
        }
        Ok(items)
    }

    pub fn list_plugin_install_summaries(&self) -> Result<Vec<PluginInstallListSummary>> {
        let sql = plugin_install_summary_list_sql();
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(PluginInstallListSummary {
                plugin_id: row.get(0)?,
                source_url: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                description: row.get(4)?,
                author: row.get(5)?,
                homepage_url: row.get(6)?,
                script_url: row.get(7)?,
                permissions_json: row.get(8)?,
                status: row.get(9)?,
                installed_at: row.get(10)?,
                updated_at: row.get(11)?,
                last_run_at: row.get(12)?,
                last_error: row.get(13)?,
                manifest_version: row.get(14)?,
                category: row.get(15)?,
                runtime_kind: row.get(16)?,
                tags_json: row.get(17)?,
            });
        }
        Ok(items)
    }

    /// 函数 `find_plugin_install`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_plugin_install(&self, plugin_id: &str) -> Result<Option<PluginInstall>> {
        let sql = plugin_install_by_id_sql(
            "
                plugin_id, source_url, name, version, description, author, homepage_url, script_url,
                script_body, permissions_json, manifest_json, status, installed_at, updated_at,
                last_run_at, last_error
            ",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([plugin_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_plugin_install_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn find_plugin_runtime_install(
        &self,
        plugin_id: &str,
    ) -> Result<Option<PluginRuntimeInstall>> {
        let sql = plugin_install_by_id_sql(
            "
                plugin_id, source_url, name, version, script_body, permissions_json, status
            ",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([plugin_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(PluginRuntimeInstall {
                plugin_id: row.get(0)?,
                source_url: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                script_body: row.get(4)?,
                permissions_json: row.get(5)?,
                status: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn plugin_install_names_for_plugins(
        &self,
        plugin_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let ids = normalize_text_ids(plugin_ids);
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut names = HashMap::new();
        for chunk in ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            let Some((condition, params)) = text_id_in_clause("plugin_id", chunk) else {
                continue;
            };
            let sql = plugin_install_names_for_plugins_chunk_sql(&condition);
            let mut stmt = self.conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                names.insert(row.get(0)?, row.get(1)?);
            }
        }
        Ok(names)
    }

    /// 函数 `update_plugin_install_status`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    /// - status: 参数 status
    /// - last_error: 参数 last_error
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_plugin_install_status(
        &self,
        plugin_id: &str,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE plugin_installs
             SET status = ?1, last_error = ?2, updated_at = ?3
             WHERE plugin_id = ?4",
            (status, last_error, now_ts(), plugin_id),
        )?;
        Ok(())
    }

    /// 函数 `update_plugin_install_last_run`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    /// - last_run_at: 参数 last_run_at
    /// - last_error: 参数 last_error
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_plugin_install_last_run(
        &self,
        plugin_id: &str,
        last_run_at: i64,
        last_error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE plugin_installs
             SET last_run_at = ?1, last_error = ?2, updated_at = ?3
             WHERE plugin_id = ?4",
            (last_run_at, last_error, now_ts(), plugin_id),
        )?;
        Ok(())
    }

    /// 函数 `delete_plugin_install`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn delete_plugin_install(&self, plugin_id: &str) -> Result<()> {
        self.conn
            .execute(delete_plugin_tasks_for_plugin_sql(), [plugin_id])?;
        self.conn
            .execute(delete_plugin_install_by_id_sql(), [plugin_id])?;
        Ok(())
    }

    /// 函数 `list_plugin_tasks`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_plugin_tasks(&self, plugin_id: Option<&str>) -> Result<Vec<PluginTask>> {
        let sql = plugin_task_list_sql(
            "id, plugin_id, name, description, entrypoint, schedule_kind, interval_seconds,
                enabled, next_run_at, last_run_at, last_status, last_error, task_json, created_at, updated_at",
            plugin_id.is_some(),
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = if let Some(plugin_id) = plugin_id {
            stmt.query([plugin_id])?
        } else {
            stmt.query([])?
        };
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_plugin_task_row(row)?);
        }
        Ok(items)
    }

    pub fn list_plugin_task_summaries(
        &self,
        plugin_id: Option<&str>,
    ) -> Result<Vec<PluginTaskListSummary>> {
        let sql = plugin_task_summary_list_sql(plugin_id.is_some());
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = if let Some(plugin_id) = plugin_id {
            stmt.query([plugin_id])?
        } else {
            stmt.query([])?
        };
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(PluginTaskListSummary {
                id: row.get(0)?,
                plugin_id: row.get(1)?,
                plugin_name: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                entrypoint: row.get(5)?,
                schedule_kind: row.get(6)?,
                interval_seconds: row.get(7)?,
                enabled: row.get(8)?,
                next_run_at: row.get(9)?,
                last_run_at: row.get(10)?,
                last_status: row.get(11)?,
                last_error: row.get(12)?,
            });
        }
        Ok(items)
    }

    pub fn plugin_task_names_for_tasks(
        &self,
        task_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let ids = normalize_text_ids(task_ids);
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut names = HashMap::new();
        for chunk in ids.chunks(SQLITE_IN_CLAUSE_BATCH_SIZE) {
            let Some((condition, params)) = text_id_in_clause("id", chunk) else {
                continue;
            };
            let sql = plugin_task_names_for_tasks_chunk_sql(&condition);
            let mut stmt = self.conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                names.insert(row.get(0)?, row.get(1)?);
            }
        }
        Ok(names)
    }

    pub fn plugin_task_counts_by_plugin(&self) -> Result<HashMap<String, PluginTaskCount>> {
        let mut stmt = self.conn.prepare(plugin_task_counts_by_plugin_sql())?;
        let mut rows = stmt.query([])?;
        let mut counts = HashMap::new();
        while let Some(row) = rows.next()? {
            let count = PluginTaskCount {
                plugin_id: row.get(0)?,
                task_count: row.get(1)?,
                enabled_task_count: row.get(2)?,
            };
            counts.insert(count.plugin_id.clone(), count);
        }
        Ok(counts)
    }

    pub fn list_plugin_tasks_needing_schedule_repair(
        &self,
        plugin_id: Option<&str>,
    ) -> Result<Vec<PluginTaskScheduleRepairRow>> {
        let sql = if plugin_id.is_some() {
            "SELECT id, interval_seconds, next_run_at, last_run_at, last_status, last_error
             FROM plugin_tasks
             WHERE plugin_id = ?1
               AND enabled = 1
               AND schedule_kind <> 'manual'
               AND next_run_at IS NULL
               AND interval_seconds IS NOT NULL
               AND interval_seconds > 0
             ORDER BY created_at ASC, id ASC"
        } else {
            "SELECT id, interval_seconds, next_run_at, last_run_at, last_status, last_error
             FROM plugin_tasks
             WHERE enabled = 1
               AND schedule_kind <> 'manual'
               AND next_run_at IS NULL
               AND interval_seconds IS NOT NULL
               AND interval_seconds > 0
             ORDER BY created_at ASC, id ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = if let Some(plugin_id) = plugin_id {
            stmt.query([plugin_id])?
        } else {
            stmt.query([])?
        };
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(PluginTaskScheduleRepairRow {
                id: row.get(0)?,
                interval_seconds: row.get(1)?,
                next_run_at: row.get(2)?,
                last_run_at: row.get(3)?,
                last_status: row.get(4)?,
                last_error: row.get(5)?,
            });
        }
        Ok(items)
    }

    pub fn repair_plugin_task_schedules(&self, plugin_id: Option<&str>, now: i64) -> Result<usize> {
        let updated_at = now_ts();
        if let Some(plugin_id) = plugin_id {
            self.conn.execute(
                repair_plugin_task_schedules_sql(true),
                (now, updated_at, plugin_id),
            )
        } else {
            self.conn
                .execute(repair_plugin_task_schedules_sql(false), (now, updated_at))
        }
    }

    /// 函数 `find_plugin_task`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - task_id: 参数 task_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_plugin_task(&self, task_id: &str) -> Result<Option<PluginTask>> {
        let sql = plugin_task_by_id_sql(
            "id, plugin_id, name, description, entrypoint, schedule_kind, interval_seconds,
                enabled, next_run_at, last_run_at, last_status, last_error, task_json, created_at, updated_at
            ",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([task_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_plugin_task_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `set_plugin_task_enabled`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - task_id: 参数 task_id
    /// - enabled: 参数 enabled
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn set_plugin_task_enabled(&self, task_id: &str, enabled: bool) -> Result<()> {
        self.conn.execute(
            update_plugin_task_enabled_sql(),
            (if enabled { 1_i64 } else { 0_i64 }, now_ts(), task_id),
        )?;
        Ok(())
    }

    /// 函数 `update_plugin_task_definition`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - task_id: 参数 task_id
    /// - name: 参数 name
    /// - description: 参数 description
    /// - entrypoint: 参数 entrypoint
    /// - schedule_kind: 参数 schedule_kind
    /// - interval_seconds: 参数 interval_seconds
    /// - enabled: 参数 enabled
    /// - next_run_at: 参数 next_run_at
    /// - task_json: 参数 task_json
    ///
    /// # 返回
    /// 返回函数执行结果
    #[allow(clippy::too_many_arguments)]
    pub fn update_plugin_task_definition(
        &self,
        task_id: &str,
        name: &str,
        description: Option<&str>,
        entrypoint: &str,
        schedule_kind: &str,
        interval_seconds: Option<i64>,
        enabled: bool,
        next_run_at: Option<i64>,
        task_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            update_plugin_task_definition_sql(),
            (
                name,
                description,
                entrypoint,
                schedule_kind,
                interval_seconds,
                if enabled { 1_i64 } else { 0_i64 },
                next_run_at,
                task_json,
                now_ts(),
                task_id,
            ),
        )?;
        Ok(())
    }

    /// 函数 `update_plugin_task_schedule`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - task_id: 参数 task_id
    /// - next_run_at: 参数 next_run_at
    /// - last_run_at: 参数 last_run_at
    /// - last_status: 参数 last_status
    /// - last_error: 参数 last_error
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_plugin_task_schedule(
        &self,
        task_id: &str,
        next_run_at: Option<i64>,
        last_run_at: Option<i64>,
        last_status: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            update_plugin_task_schedule_sql(),
            (
                next_run_at,
                last_run_at,
                last_status,
                last_error,
                now_ts(),
                task_id,
            ),
        )?;
        Ok(())
    }

    /// 函数 `list_due_plugin_tasks`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - now: 参数 now
    /// - limit: 参数 limit
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_due_plugin_tasks(
        &self,
        now: i64,
        limit: i64,
    ) -> Result<Vec<PluginTaskExecutionRow>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }

        let sql = due_plugin_tasks_sql(
            "t.id, t.plugin_id, t.name, t.description, t.entrypoint, t.schedule_kind, t.interval_seconds,
                t.enabled",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![now, limit])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(PluginTaskExecutionRow {
                id: row.get(0)?,
                plugin_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                entrypoint: row.get(4)?,
                schedule_kind: row.get(5)?,
                interval_seconds: row.get(6)?,
                enabled: row.get::<_, i64>(7)? != 0,
            });
        }
        Ok(items)
    }

    pub fn next_enabled_plugin_task_run_at(&self) -> Result<Option<i64>> {
        self.conn
            .query_row(next_enabled_plugin_task_run_at_sql(), [], |row| row.get(0))
    }

    /// 函数 `insert_plugin_run_log`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - log: 参数 log
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn insert_plugin_run_log(&self, log: &PluginRunLog) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO plugin_run_logs (
                plugin_id, task_id, run_type, status, started_at, finished_at, duration_ms, output_json, error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &log.plugin_id,
                &log.task_id,
                &log.run_type,
                &log.status,
                log.started_at,
                &log.finished_at,
                &log.duration_ms,
                &log.output_json,
                &log.error,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 函数 `list_plugin_run_logs`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - plugin_id: 参数 plugin_id
    /// - task_id: 参数 task_id
    /// - limit: 参数 limit
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_plugin_run_logs(
        &self,
        plugin_id: Option<&str>,
        task_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PluginRunLog>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }

        let mut params = Vec::new();
        if let Some(plugin_id) = plugin_id {
            params.push(Value::Text(plugin_id.to_string()));
        }
        if let Some(task_id) = task_id {
            params.push(Value::Text(task_id.to_string()));
        }
        let sql = plugin_run_log_list_sql(plugin_id.is_some(), task_id.is_some());
        params.push(Value::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(params))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_plugin_run_log_row(row)?);
        }
        Ok(items)
    }

    pub fn list_plugin_run_log_summaries(
        &self,
        plugin_id: Option<&str>,
        task_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PluginRunLogListSummary>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }

        let mut params = Vec::new();
        if let Some(plugin_id) = plugin_id {
            params.push(Value::Text(plugin_id.to_string()));
        }
        if let Some(task_id) = task_id {
            params.push(Value::Text(task_id.to_string()));
        }
        let sql = plugin_run_log_summary_list_sql(plugin_id.is_some(), task_id.is_some());
        params.push(Value::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(params))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(PluginRunLogListSummary {
                id: row.get(0)?,
                plugin_id: row.get(1)?,
                plugin_name: row.get(2)?,
                task_id: row.get(3)?,
                task_name: row.get(4)?,
                run_type: row.get(5)?,
                status: row.get(6)?,
                started_at: row.get(7)?,
                finished_at: row.get(8)?,
                duration_ms: row.get(9)?,
                output_json: row.get(10)?,
                error: row.get(11)?,
            });
        }
        Ok(items)
    }
}

/// 函数 `map_plugin_install_row`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - row: 参数 row
///
/// # 返回
/// 返回函数执行结果
fn map_plugin_install_row(row: &Row<'_>) -> Result<PluginInstall> {
    Ok(PluginInstall {
        plugin_id: row.get(0)?,
        source_url: row.get(1)?,
        name: row.get(2)?,
        version: row.get(3)?,
        description: row.get(4)?,
        author: row.get(5)?,
        homepage_url: row.get(6)?,
        script_url: row.get(7)?,
        script_body: row.get(8)?,
        permissions_json: row.get(9)?,
        manifest_json: row.get(10)?,
        status: row.get(11)?,
        installed_at: row.get(12)?,
        updated_at: row.get(13)?,
        last_run_at: row.get(14)?,
        last_error: row.get(15)?,
    })
}

/// 函数 `map_plugin_task_row`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - row: 参数 row
///
/// # 返回
/// 返回函数执行结果
fn map_plugin_task_row(row: &Row<'_>) -> Result<PluginTask> {
    Ok(PluginTask {
        id: row.get(0)?,
        plugin_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        entrypoint: row.get(4)?,
        schedule_kind: row.get(5)?,
        interval_seconds: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        next_run_at: row.get(8)?,
        last_run_at: row.get(9)?,
        last_status: row.get(10)?,
        last_error: row.get(11)?,
        task_json: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

/// 函数 `map_plugin_run_log_row`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - row: 参数 row
///
/// # 返回
/// 返回函数执行结果
fn map_plugin_run_log_row(row: &Row<'_>) -> Result<PluginRunLog> {
    Ok(PluginRunLog {
        id: row.get(0)?,
        plugin_id: row.get(1)?,
        task_id: row.get(2)?,
        run_type: row.get(3)?,
        status: row.get(4)?,
        started_at: row.get(5)?,
        finished_at: row.get(6)?,
        duration_ms: row.get(7)?,
        output_json: row.get(8)?,
        error: row.get(9)?,
    })
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
