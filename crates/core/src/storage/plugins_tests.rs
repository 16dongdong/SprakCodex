use super::super::{PluginInstall, PluginRunLog, PluginTask, Storage};
use super::{
    delete_plugin_install_by_id_sql, delete_plugin_tasks_for_plugin_sql, due_plugin_tasks_sql,
    next_enabled_plugin_task_run_at_sql, plugin_install_by_id_sql,
    plugin_install_names_for_plugins_chunk_sql, plugin_install_summary_list_sql,
    plugin_run_log_list_sql, plugin_run_log_summary_list_sql, plugin_task_by_id_sql,
    plugin_task_counts_by_plugin_sql, plugin_task_list_sql, plugin_task_names_for_tasks_chunk_sql,
    repair_plugin_task_schedules_sql, update_plugin_task_definition_sql,
    update_plugin_task_enabled_sql, update_plugin_task_schedule_sql,
};

fn plugin_install(plugin_id: &str, status: &str) -> PluginInstall {
    PluginInstall {
        plugin_id: plugin_id.to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: plugin_id.to_string(),
        version: "1.0.0".to_string(),
        description: Some("test".to_string()),
        author: Some("CodexManager".to_string()),
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { context }".to_string(),
        permissions_json: serde_json::json!([]).to_string(),
        manifest_json: serde_json::json!({ "id": plugin_id }).to_string(),
        status: status.to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    }
}

fn plugin_task(
    plugin_id: &str,
    task_id: &str,
    schedule_kind: &str,
    enabled: bool,
    next_run_at: Option<i64>,
) -> PluginTask {
    PluginTask {
        id: format!("{plugin_id}::{task_id}"),
        plugin_id: plugin_id.to_string(),
        name: task_id.to_string(),
        description: Some("test".to_string()),
        entrypoint: task_id.to_string(),
        schedule_kind: schedule_kind.to_string(),
        interval_seconds: if schedule_kind == "interval" {
            Some(60)
        } else {
            None
        },
        enabled,
        next_run_at,
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": task_id,
            "name": task_id,
            "entrypoint": task_id,
            "scheduleKind": schedule_kind,
            "enabled": enabled
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    }
}

/// 函数 `update_plugin_task_definition_updates_interval`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn update_plugin_task_definition_updates_interval() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let install = PluginInstall {
        plugin_id: "cleanup-banned-accounts".to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: "清理封禁账号".to_string(),
        version: "1.0.0".to_string(),
        description: Some("test".to_string()),
        author: Some("CodexManager".to_string()),
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { context }".to_string(),
        permissions_json: serde_json::json!(["accounts:cleanup"]).to_string(),
        manifest_json: serde_json::json!({ "id": "cleanup-banned-accounts" }).to_string(),
        status: "enabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let task = PluginTask {
        id: "cleanup-banned-accounts::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "手动清理".to_string(),
        description: Some("click".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "manual".to_string(),
        interval_seconds: None,
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "手动清理",
            "entrypoint": "run",
            "scheduleKind": "manual",
            "enabled": true
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    };

    storage
        .replace_plugin_install(&install, &[task])
        .expect("seed plugin");
    storage
        .update_plugin_task_definition(
            "cleanup-banned-accounts::run",
            "定时自动清理",
            Some("每 60 秒自动清理一次所有封禁账号"),
            "run",
            "interval",
            Some(60),
            true,
            Some(61),
            &serde_json::json!({
                "id": "run",
                "name": "定时自动清理",
                "entrypoint": "run",
                "scheduleKind": "interval",
                "intervalSeconds": 60,
                "enabled": true
            })
            .to_string(),
        )
        .expect("update task");

    let updated = storage
        .find_plugin_task("cleanup-banned-accounts::run")
        .expect("read task")
        .expect("task exists");
    assert_eq!(updated.schedule_kind, "interval");
    assert_eq!(updated.interval_seconds, Some(60));
    assert_eq!(updated.next_run_at, Some(61));
}

/// 函数 `list_due_plugin_tasks_returns_enabled_interval_tasks`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn list_due_plugin_tasks_returns_enabled_interval_tasks() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let install = PluginInstall {
        plugin_id: "cleanup-banned-accounts".to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: "清理封禁账号".to_string(),
        version: "1.0.0".to_string(),
        description: Some("test".to_string()),
        author: Some("CodexManager".to_string()),
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { context }".to_string(),
        permissions_json: serde_json::json!(["accounts:cleanup"]).to_string(),
        manifest_json: serde_json::json!({ "id": "cleanup-banned-accounts" }).to_string(),
        status: "enabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let task = PluginTask {
        id: "cleanup-banned-accounts::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "定时自动清理".to_string(),
        description: Some("auto".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "interval".to_string(),
        interval_seconds: Some(60),
        enabled: true,
        next_run_at: Some(10),
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "定时自动清理",
            "entrypoint": "run",
            "scheduleKind": "interval",
            "intervalSeconds": 60,
            "enabled": true
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    };

    storage
        .replace_plugin_install(&install, &[task])
        .expect("seed plugin");

    let due = storage
        .list_due_plugin_tasks(100, 10)
        .expect("list due tasks");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "cleanup-banned-accounts::run");
    assert_eq!(due[0].plugin_id, "cleanup-banned-accounts");
    assert_eq!(due[0].name, "定时自动清理");
    assert_eq!(due[0].description.as_deref(), Some("auto"));
    assert_eq!(due[0].entrypoint, "run");
    assert_eq!(due[0].schedule_kind, "interval");
    assert_eq!(due[0].interval_seconds, Some(60));
    assert!(due[0].enabled);
}

#[test]
fn list_due_plugin_tasks_short_circuits_non_positive_limit() {
    let storage = Storage::open_in_memory().expect("open storage");

    let zero_limit = storage
        .list_due_plugin_tasks(100, 0)
        .expect("zero limit should not query storage");
    let negative_limit = storage
        .list_due_plugin_tasks(100, -1)
        .expect("negative limit should not query storage");

    assert!(zero_limit.is_empty());
    assert!(negative_limit.is_empty());
}

#[test]
fn plugin_name_helpers_filter_to_requested_ids() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let install = PluginInstall {
        plugin_id: "cleanup-banned-accounts".to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: "清理封禁账号".to_string(),
        version: "1.0.0".to_string(),
        description: Some("test".to_string()),
        author: Some("CodexManager".to_string()),
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { context }".to_string(),
        permissions_json: serde_json::json!(["accounts:cleanup"]).to_string(),
        manifest_json: serde_json::json!({ "id": "cleanup-banned-accounts" }).to_string(),
        status: "enabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let task = PluginTask {
        id: "cleanup-banned-accounts::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "定时自动清理".to_string(),
        description: Some("auto".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "interval".to_string(),
        interval_seconds: Some(60),
        enabled: true,
        next_run_at: Some(10),
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "定时自动清理",
            "entrypoint": "run",
            "scheduleKind": "interval",
            "intervalSeconds": 60,
            "enabled": true
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    };

    storage
        .replace_plugin_install(&install, &[task])
        .expect("seed plugin");

    let plugin_names = storage
        .plugin_install_names_for_plugins(&[
            "cleanup-banned-accounts".to_string(),
            "cleanup-banned-accounts".to_string(),
            "missing-plugin".to_string(),
            " ".to_string(),
        ])
        .expect("load plugin names");
    assert_eq!(plugin_names.len(), 1);
    assert_eq!(
        plugin_names
            .get("cleanup-banned-accounts")
            .map(String::as_str),
        Some("清理封禁账号")
    );

    let task_names = storage
        .plugin_task_names_for_tasks(&[
            "cleanup-banned-accounts::run".to_string(),
            "missing-task".to_string(),
        ])
        .expect("load task names");
    assert_eq!(task_names.len(), 1);
    assert_eq!(
        task_names
            .get("cleanup-banned-accounts::run")
            .map(String::as_str),
        Some("定时自动清理")
    );
}

#[test]
fn list_plugin_task_summaries_join_plugin_names_and_skip_task_json() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut first_install = plugin_install("first-plugin", "enabled");
    first_install.name = "First Plugin".to_string();
    let mut second_install = plugin_install("second-plugin", "enabled");
    second_install.name = "Second Plugin".to_string();

    let mut first_task = plugin_task("first-plugin", "run", "interval", true, Some(20));
    first_task.name = "Run First".to_string();
    first_task.description = Some("first summary".to_string());
    first_task.task_json = "x".repeat(4096);
    first_task.created_at = 2;
    let mut second_task = plugin_task("second-plugin", "run", "manual", false, None);
    second_task.name = "Run Second".to_string();
    second_task.created_at = 1;

    storage
        .replace_plugin_install(&first_install, &[first_task])
        .expect("seed first plugin");
    storage
        .replace_plugin_install(&second_install, &[second_task])
        .expect("seed second plugin");

    let all = storage
        .list_plugin_task_summaries(None)
        .expect("list all task summaries");
    assert_eq!(
        all.iter()
            .map(|item| (item.plugin_id.as_str(), item.plugin_name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("second-plugin", "Second Plugin"),
            ("first-plugin", "First Plugin")
        ]
    );
    assert_eq!(all[1].id, "first-plugin::run");
    assert_eq!(all[1].name, "Run First");
    assert_eq!(all[1].description.as_deref(), Some("first summary"));
    assert_eq!(all[1].entrypoint, "run");
    assert_eq!(all[1].schedule_kind, "interval");
    assert_eq!(all[1].interval_seconds, Some(60));
    assert!(all[1].enabled);
    assert_eq!(all[1].next_run_at, Some(20));
    assert_eq!(all[1].last_status, None);

    let first_only = storage
        .list_plugin_task_summaries(Some("first-plugin"))
        .expect("list first task summaries");
    assert_eq!(first_only.len(), 1);
    assert_eq!(first_only[0].plugin_id, "first-plugin");
    assert_eq!(first_only[0].plugin_name, "First Plugin");
}

#[test]
fn list_due_plugin_tasks_treats_missing_next_run_as_due() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let install = PluginInstall {
        plugin_id: "always-due-plugin".to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: "Always Due".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        author: None,
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { context }".to_string(),
        permissions_json: serde_json::json!([]).to_string(),
        manifest_json: serde_json::json!({ "id": "always-due-plugin" }).to_string(),
        status: "enabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let task = PluginTask {
        id: "always-due-plugin::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "Run".to_string(),
        description: None,
        entrypoint: "run".to_string(),
        schedule_kind: "interval".to_string(),
        interval_seconds: Some(60),
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "Run",
            "entrypoint": "run",
            "scheduleKind": "interval",
            "intervalSeconds": 60,
            "enabled": true
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    };

    storage
        .replace_plugin_install(&install, &[task])
        .expect("seed plugin");

    let due = storage
        .list_due_plugin_tasks(100, 10)
        .expect("list due tasks");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "always-due-plugin::run");
}

#[test]
fn next_enabled_plugin_task_run_at_returns_minimum_enabled_interval_task() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .replace_plugin_install(
            &plugin_install("enabled-plugin", "enabled"),
            &[
                plugin_task("enabled-plugin", "later", "interval", true, Some(50)),
                plugin_task("enabled-plugin", "manual", "manual", true, Some(5)),
                plugin_task("enabled-plugin", "disabled", "interval", false, Some(3)),
                plugin_task("enabled-plugin", "unscheduled", "interval", true, None),
                plugin_task("enabled-plugin", "earliest", "interval", true, Some(20)),
            ],
        )
        .expect("seed enabled plugin");
    storage
        .replace_plugin_install(
            &plugin_install("disabled-plugin", "disabled"),
            &[plugin_task(
                "disabled-plugin",
                "earlier",
                "interval",
                true,
                Some(10),
            )],
        )
        .expect("seed disabled plugin");

    let next_run_at = storage
        .next_enabled_plugin_task_run_at()
        .expect("read next run time");
    assert_eq!(next_run_at, Some(20));
}

#[test]
fn next_enabled_plugin_task_run_at_returns_none_without_schedulable_tasks() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .replace_plugin_install(
            &plugin_install("manual-only-plugin", "enabled"),
            &[
                plugin_task("manual-only-plugin", "manual", "manual", true, Some(5)),
                plugin_task(
                    "manual-only-plugin",
                    "disabled",
                    "interval",
                    false,
                    Some(10),
                ),
                plugin_task("manual-only-plugin", "unscheduled", "interval", true, None),
            ],
        )
        .expect("seed plugin");

    let next_run_at = storage
        .next_enabled_plugin_task_run_at()
        .expect("read next run time");
    assert_eq!(next_run_at, None);
}

#[test]
fn plugin_task_counts_by_plugin_groups_task_totals() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .replace_plugin_install(
            &plugin_install("first-plugin", "enabled"),
            &[
                plugin_task("first-plugin", "enabled-a", "interval", true, Some(10)),
                plugin_task("first-plugin", "enabled-b", "manual", true, None),
                plugin_task("first-plugin", "disabled", "interval", false, Some(20)),
            ],
        )
        .expect("seed first plugin");
    storage
        .replace_plugin_install(
            &plugin_install("second-plugin", "enabled"),
            &[plugin_task(
                "second-plugin",
                "enabled",
                "interval",
                true,
                Some(30),
            )],
        )
        .expect("seed second plugin");

    let counts = storage
        .plugin_task_counts_by_plugin()
        .expect("read task counts");
    let first = counts.get("first-plugin").expect("first count");
    assert_eq!(first.task_count, 3);
    assert_eq!(first.enabled_task_count, 2);
    let second = counts.get("second-plugin").expect("second count");
    assert_eq!(second.task_count, 1);
    assert_eq!(second.enabled_task_count, 1);
    assert!(!counts.contains_key("missing-plugin"));
}

#[test]
fn schedule_repair_rows_return_only_enabled_unscheduled_interval_tasks() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut no_interval = plugin_task("first-plugin", "no-interval", "interval", true, None);
    no_interval.interval_seconds = None;
    let mut no_positive_interval =
        plugin_task("first-plugin", "zero-interval", "interval", true, None);
    no_positive_interval.interval_seconds = Some(0);

    storage
        .replace_plugin_install(
            &plugin_install("first-plugin", "enabled"),
            &[
                plugin_task("first-plugin", "repair", "interval", true, None),
                plugin_task("first-plugin", "manual", "manual", true, None),
                plugin_task("first-plugin", "disabled", "interval", false, None),
                plugin_task("first-plugin", "already-set", "interval", true, Some(20)),
                no_interval,
                no_positive_interval,
            ],
        )
        .expect("seed first plugin");
    storage
        .replace_plugin_install(
            &plugin_install("second-plugin", "enabled"),
            &[plugin_task(
                "second-plugin",
                "repair",
                "interval",
                true,
                None,
            )],
        )
        .expect("seed second plugin");

    let first_rows = storage
        .list_plugin_tasks_needing_schedule_repair(Some("first-plugin"))
        .expect("read first plugin repair rows");
    assert_eq!(first_rows.len(), 1);
    assert_eq!(first_rows[0].id, "first-plugin::repair");
    assert_eq!(first_rows[0].interval_seconds, Some(60));
    assert_eq!(first_rows[0].next_run_at, None);

    let all_rows = storage
        .list_plugin_tasks_needing_schedule_repair(None)
        .expect("read all repair rows");
    assert_eq!(
        all_rows
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["first-plugin::repair", "second-plugin::repair"]
    );
}

#[test]
fn repair_plugin_task_schedules_batches_enabled_unscheduled_interval_tasks() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut from_last_run = plugin_task("first-plugin", "from-last-run", "interval", true, None);
    from_last_run.last_run_at = Some(900);
    from_last_run.last_status = Some("ok".to_string());
    let uses_now = plugin_task("first-plugin", "uses-now", "interval", true, None);
    let mut no_positive_interval =
        plugin_task("first-plugin", "zero-interval", "interval", true, None);
    no_positive_interval.interval_seconds = Some(0);

    storage
        .replace_plugin_install(
            &plugin_install("first-plugin", "enabled"),
            &[
                from_last_run,
                uses_now,
                plugin_task("first-plugin", "manual", "manual", true, None),
                plugin_task("first-plugin", "disabled", "interval", false, None),
                plugin_task("first-plugin", "already-set", "interval", true, Some(20)),
                no_positive_interval,
            ],
        )
        .expect("seed first plugin");
    storage
        .replace_plugin_install(
            &plugin_install("second-plugin", "enabled"),
            &[plugin_task(
                "second-plugin",
                "still-unscheduled",
                "interval",
                true,
                None,
            )],
        )
        .expect("seed second plugin");

    let repaired = storage
        .repair_plugin_task_schedules(Some("first-plugin"), 1000)
        .expect("repair first plugin schedules");
    assert_eq!(repaired, 2);

    let from_last_run = storage
        .find_plugin_task("first-plugin::from-last-run")
        .expect("read from last run")
        .expect("task exists");
    assert_eq!(from_last_run.next_run_at, Some(960));
    assert_eq!(from_last_run.last_run_at, Some(900));
    assert_eq!(from_last_run.last_status.as_deref(), Some("ok"));

    let uses_now = storage
        .find_plugin_task("first-plugin::uses-now")
        .expect("read uses now")
        .expect("task exists");
    assert_eq!(uses_now.next_run_at, Some(1000));

    for task_id in [
        "first-plugin::manual",
        "first-plugin::disabled",
        "first-plugin::zero-interval",
        "second-plugin::still-unscheduled",
    ] {
        let task = storage
            .find_plugin_task(task_id)
            .expect("read skipped task")
            .expect("task exists");
        assert_eq!(task.next_run_at, None, "{task_id} should stay unscheduled");
    }

    let already_set = storage
        .find_plugin_task("first-plugin::already-set")
        .expect("read already set")
        .expect("task exists");
    assert_eq!(already_set.next_run_at, Some(20));

    let repaired_all = storage
        .repair_plugin_task_schedules(None, 2000)
        .expect("repair all plugin schedules");
    assert_eq!(repaired_all, 1);
    let second = storage
        .find_plugin_task("second-plugin::still-unscheduled")
        .expect("read second task")
        .expect("task exists");
    assert_eq!(second.next_run_at, Some(2000));
}

#[test]
fn list_plugin_install_summaries_skip_script_body_and_manifest_blob() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .replace_plugin_install(
            &PluginInstall {
                plugin_id: "summary-plugin".to_string(),
                source_url: Some("builtin://codexmanager".to_string()),
                name: "Summary Plugin".to_string(),
                version: "1.2.3".to_string(),
                description: Some("summary".to_string()),
                author: Some("CodexManager".to_string()),
                homepage_url: Some("https://example.test/plugin".to_string()),
                script_url: Some("https://example.test/plugin.rhai".to_string()),
                script_body: "x".repeat(4096),
                permissions_json: serde_json::json!(["logs:read"]).to_string(),
                manifest_json: serde_json::json!({
                    "id": "summary-plugin",
                    "manifestVersion": "2",
                    "category": "official",
                    "runtimeKind": "rhai",
                    "tags": ["maintenance", "logs"],
                    "scriptBody": "ignored large manifest field"
                })
                .to_string(),
                status: "enabled".to_string(),
                installed_at: 1,
                updated_at: 2,
                last_run_at: Some(3),
                last_error: None,
            },
            &[],
        )
        .expect("seed plugin");

    let summaries = storage
        .list_plugin_install_summaries()
        .expect("read plugin summaries");

    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(summary.plugin_id, "summary-plugin");
    assert_eq!(
        summary.permissions_json,
        serde_json::json!(["logs:read"]).to_string()
    );
    assert_eq!(summary.manifest_version.as_deref(), Some("2"));
    assert_eq!(summary.category.as_deref(), Some("official"));
    assert_eq!(summary.runtime_kind.as_deref(), Some("rhai"));
    assert_eq!(
        summary.tags_json.as_deref(),
        Some(
            serde_json::json!(["maintenance", "logs"])
                .to_string()
                .as_str()
        )
    );
}

#[test]
fn runtime_install_lookup_reads_only_runtime_fields() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    storage
        .replace_plugin_install(
            &PluginInstall {
                plugin_id: "runtime-plugin".to_string(),
                source_url: Some("builtin://codexmanager".to_string()),
                name: "Runtime Plugin".to_string(),
                version: "1.2.3".to_string(),
                description: Some("display-only description".to_string()),
                author: Some("CodexManager".to_string()),
                homepage_url: Some("https://example.test/plugin".to_string()),
                script_url: Some("https://example.test/plugin.rhai".to_string()),
                script_body: "fn run(context) { context }".to_string(),
                permissions_json: serde_json::json!(["settings:read"]).to_string(),
                manifest_json: serde_json::json!({
                    "id": "runtime-plugin",
                    "description": "large display metadata",
                    "tags": ["runtime"]
                })
                .to_string(),
                status: "enabled".to_string(),
                installed_at: 1,
                updated_at: 2,
                last_run_at: Some(3),
                last_error: Some("old error".to_string()),
            },
            &[],
        )
        .expect("seed plugin");

    let plugin = storage
        .find_plugin_runtime_install("runtime-plugin")
        .expect("read runtime plugin")
        .expect("runtime plugin");

    assert_eq!(plugin.plugin_id, "runtime-plugin");
    assert_eq!(plugin.source_url.as_deref(), Some("builtin://codexmanager"));
    assert_eq!(plugin.name, "Runtime Plugin");
    assert_eq!(plugin.version, "1.2.3");
    assert_eq!(plugin.script_body, "fn run(context) { context }");
    assert_eq!(
        plugin.permissions_json,
        serde_json::json!(["settings:read"]).to_string()
    );
    assert_eq!(plugin.status, "enabled");
}

#[test]
fn list_plugin_run_log_summaries_joins_display_names() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut install = plugin_install("summary-plugin", "enabled");
    install.name = "Summary Plugin".to_string();
    let mut task = plugin_task("summary-plugin", "sync", "manual", true, None);
    task.name = "Sync Task".to_string();
    storage
        .replace_plugin_install(&install, &[task])
        .expect("seed plugin");
    storage
        .insert_plugin_run_log(&PluginRunLog {
            id: None,
            plugin_id: "summary-plugin".to_string(),
            task_id: Some("summary-plugin::sync".to_string()),
            run_type: "manual".to_string(),
            status: "ok".to_string(),
            started_at: 100,
            finished_at: Some(110),
            duration_ms: Some(10),
            output_json: Some(serde_json::json!({ "ok": true }).to_string()),
            error: None,
        })
        .expect("insert joined log");
    storage
        .insert_plugin_run_log(&PluginRunLog {
            id: None,
            plugin_id: "missing-plugin".to_string(),
            task_id: Some("missing-plugin::sync".to_string()),
            run_type: "manual".to_string(),
            status: "error".to_string(),
            started_at: 200,
            finished_at: Some(205),
            duration_ms: Some(5),
            output_json: None,
            error: Some("missing".to_string()),
        })
        .expect("insert orphan log");

    let logs = storage
        .list_plugin_run_log_summaries(None, None, 10)
        .expect("list log summaries");

    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].plugin_id, "missing-plugin");
    assert_eq!(logs[0].plugin_name, None);
    assert_eq!(logs[0].task_name, None);
    assert_eq!(logs[0].error.as_deref(), Some("missing"));
    assert_eq!(logs[1].plugin_id, "summary-plugin");
    assert_eq!(logs[1].plugin_name.as_deref(), Some("Summary Plugin"));
    assert_eq!(logs[1].task_id.as_deref(), Some("summary-plugin::sync"));
    assert_eq!(logs[1].task_name.as_deref(), Some("Sync Task"));
    assert_eq!(
        logs[1].output_json.as_deref(),
        Some(serde_json::json!({ "ok": true }).to_string().as_str())
    );
}

#[test]
fn list_plugin_run_logs_short_circuit_non_positive_limits() {
    let storage = Storage::open_in_memory().expect("open storage");

    let raw_logs = storage
        .list_plugin_run_logs(None, None, 0)
        .expect("zero raw log limit should not query storage");
    let summaries = storage
        .list_plugin_run_log_summaries(None, None, -1)
        .expect("negative summary limit should not query storage");

    assert!(raw_logs.is_empty());
    assert!(summaries.is_empty());
}

#[test]
fn plugin_task_counts_by_plugin_uses_plugin_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            plugin_task_counts_by_plugin_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_plugin_tasks_plugin_id_enabled_next_run_at"),
        "expected plugin task plugin lookup index in plan, got {plan}"
    );
}

#[test]
fn list_plugin_install_summaries_uses_list_order_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            plugin_install_summary_list_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_plugin_installs_list_order"),
        "expected plugin install list-order index in plan, got {plan}"
    );
}

#[test]
fn plugin_install_direct_lookup_helpers_use_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let full_install_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_install_by_id_sql("plugin_id, name, script_body")
    );
    let full_install_plan = collect_query_plan(&storage, &full_install_sql, ["plugin-a"]);
    assert!(
        full_install_plan.contains("sqlite_autoindex_plugin_installs_1"),
        "expected plugin install full lookup to use primary-key index, got {full_install_plan}"
    );

    let runtime_install_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_install_by_id_sql(
            "plugin_id, source_url, name, version, script_body, permissions_json, status"
        )
    );
    let runtime_install_plan = collect_query_plan(&storage, &runtime_install_sql, ["plugin-a"]);
    assert!(
        runtime_install_plan.contains("sqlite_autoindex_plugin_installs_1"),
        "expected plugin runtime install lookup to use primary-key index, got {runtime_install_plan}"
    );
}
#[test]
fn plugin_install_delete_helpers_use_existing_lookup_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let task_delete_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        delete_plugin_tasks_for_plugin_sql()
    );
    let task_delete_plan = collect_query_plan(&storage, &task_delete_sql, ["plugin-a"]);
    assert!(
        task_delete_plan.contains("idx_plugin_tasks_plugin_id_enabled_next_run_at")
            || task_delete_plan.contains("idx_plugin_tasks_plugin_list_order"),
        "expected plugin task cleanup to use a plugin_id lookup index, got {task_delete_plan}"
    );

    let install_delete_sql = format!("EXPLAIN QUERY PLAN {}", delete_plugin_install_by_id_sql());
    let install_delete_plan = collect_query_plan(&storage, &install_delete_sql, ["plugin-a"]);
    assert!(
        install_delete_plan.contains("sqlite_autoindex_plugin_installs_1"),
        "expected plugin install delete to use primary-key index, got {install_delete_plan}"
    );
}

#[test]
fn plugin_install_names_for_plugins_uses_plugin_id_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = plugin_install_names_for_plugins_chunk_sql("plugin_id IN (?1, ?2)");
    let mut stmt = storage
        .conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("prepare explain");
    let mut rows = stmt.query(("plugin-a", "plugin-b")).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("sqlite_autoindex_plugin_installs_1"),
        "expected plugin install primary-key lookup in plan, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "expected plugin install name chunk lookup to avoid temp sorting, got {plan}"
    );
}

#[test]
fn list_plugin_tasks_uses_list_order_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let global_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_task_list_sql("id, plugin_id", false)
    );
    let global_plan = collect_query_plan(&storage, &global_sql, []);
    assert_plan_uses_index_without_temp_sort(
        &global_plan,
        "idx_plugin_tasks_list_order",
        "global plugin task list",
    );

    let plugin_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_task_list_sql("id, plugin_id", true)
    );
    let plugin_plan = collect_query_plan(&storage, &plugin_sql, ["plugin-a"]);
    assert_plan_uses_index_without_temp_sort(
        &plugin_plan,
        "idx_plugin_tasks_plugin_list_order",
        "per-plugin task list",
    );
}

fn collect_query_plan<P>(storage: &Storage, sql: &str, params: P) -> String
where
    P: rusqlite::Params,
{
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query(params).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }
    plan
}

fn assert_plan_uses_index_without_temp_sort(plan: &str, expected_index: &str, label: &str) {
    assert!(
        plan.contains(expected_index),
        "expected {label} to use {expected_index}, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "expected {label} to avoid temp sorting, got {plan}"
    );
}

#[test]
fn plugin_task_names_for_tasks_uses_task_id_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = plugin_task_names_for_tasks_chunk_sql("id IN (?1, ?2)");
    let mut stmt = storage
        .conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("prepare explain");
    let mut rows = stmt
        .query(("plugin-a::sync", "plugin-b::sync"))
        .expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("sqlite_autoindex_plugin_tasks_1"),
        "expected plugin task primary-key lookup in plan, got {plan}"
    );
    assert!(
        !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
        "expected plugin task name chunk lookup to avoid temp sorting, got {plan}"
    );
}

#[test]
fn plugin_task_direct_lookup_helper_uses_primary_key_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_task_by_id_sql("id, plugin_id, name, task_json")
    );
    let plan = collect_query_plan(&storage, &sql, ["plugin-a::sync"]);

    assert!(
        plan.contains("sqlite_autoindex_plugin_tasks_1"),
        "expected plugin task direct lookup to use primary-key index, got {plan}"
    );
}

#[test]
fn plugin_task_write_helpers_use_expected_indexes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for (label, sql, plan) in [
        (
            "enabled update",
            update_plugin_task_enabled_sql(),
            collect_query_plan(
                &storage,
                &format!("EXPLAIN QUERY PLAN {}", update_plugin_task_enabled_sql()),
                (1_i64, 2_i64, "plugin-a::sync"),
            ),
        ),
        (
            "definition update",
            update_plugin_task_definition_sql(),
            collect_query_plan(
                &storage,
                &format!("EXPLAIN QUERY PLAN {}", update_plugin_task_definition_sql()),
                rusqlite::params![
                    "task",
                    "description",
                    "entrypoint",
                    "interval",
                    60_i64,
                    1_i64,
                    100_i64,
                    "{}",
                    200_i64,
                    "plugin-a::sync"
                ],
            ),
        ),
        (
            "schedule update",
            update_plugin_task_schedule_sql(),
            collect_query_plan(
                &storage,
                &format!("EXPLAIN QUERY PLAN {}", update_plugin_task_schedule_sql()),
                (
                    Some(100_i64),
                    Some(90_i64),
                    Some("ok"),
                    Option::<&str>::None,
                    200_i64,
                    "plugin-a::sync",
                ),
            ),
        ),
    ] {
        assert!(
            plan.contains("sqlite_autoindex_plugin_tasks_1"),
            "expected plugin task {label} helper {sql} to use primary-key index, got {plan}"
        );
    }

    let scoped_repair_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            repair_plugin_task_schedules_sql(true)
        ),
        (100_i64, 200_i64, "plugin-a"),
    );
    assert!(
        scoped_repair_plan.contains("idx_plugin_tasks_plugin_id_enabled_next_run_at")
            || scoped_repair_plan.contains("idx_plugin_tasks_plugin_list_order"),
        "expected scoped schedule repair to use plugin lookup index, got {scoped_repair_plan}"
    );

    let global_repair_plan = collect_query_plan(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            repair_plugin_task_schedules_sql(false)
        ),
        (100_i64, 200_i64),
    );
    assert!(
        global_repair_plan.contains("idx_plugin_tasks_due_lookup")
            || global_repair_plan.contains("idx_plugin_tasks_list_order"),
        "expected global schedule repair to use schedule lookup index, got {global_repair_plan}"
    );
}

#[test]
fn list_due_plugin_tasks_uses_due_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            due_plugin_tasks_sql("t.id")
        ))
        .expect("prepare explain");
    let mut rows = stmt.query((100_i64, 10_i64)).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_plugin_tasks_due_lookup"),
        "expected due lookup index in plan, got {plan}"
    );
}

#[test]
fn next_enabled_plugin_task_run_at_uses_due_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let mut stmt = storage
        .conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            next_enabled_plugin_task_run_at_sql()
        ))
        .expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let detail: String = row.get(3).expect("plan detail");
        plan.push_str(&detail);
        plan.push('\n');
    }

    assert!(
        plan.contains("idx_plugin_tasks_due_lookup"),
        "expected due lookup index in plan, got {plan}"
    );
}

#[test]
fn list_plugin_run_logs_for_task_uses_task_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_run_log_list_sql(false, true)
    );
    let plan = collect_query_plan(&storage, &sql, ("plugin-a::sync", 50_i64));

    assert!(
        plan.contains("idx_plugin_run_logs_task_lookup"),
        "expected plugin run log task lookup index in plan, got {plan}"
    );
}

#[test]
fn list_plugin_run_log_summaries_for_task_uses_task_lookup_index() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        plugin_run_log_summary_list_sql(false, true)
    );
    let plan = collect_query_plan(&storage, &sql, ("plugin-a::sync", 50_i64));

    assert!(
        plan.contains("idx_plugin_run_logs_task_lookup"),
        "expected plugin run log summary task lookup index in plan, got {plan}"
    );
}
