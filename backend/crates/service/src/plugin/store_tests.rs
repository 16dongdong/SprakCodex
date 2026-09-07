use super::{rearm_enabled_interval_tasks_for_plugin, Storage};
use crate::plugin::runtime;
use codexmanager_core::storage::{PluginInstall, PluginTask, PluginTaskExecutionRow};

/// 函数 `rearm_enabled_interval_tasks_updates_enabled_interval_tasks`
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
fn rearm_enabled_interval_tasks_updates_enabled_interval_tasks() {
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
        status: "disabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let interval_task = PluginTask {
        id: "cleanup-banned-accounts::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "自动清理".to_string(),
        description: Some("interval".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "interval".to_string(),
        interval_seconds: Some(60),
        enabled: true,
        next_run_at: None,
        last_run_at: Some(900),
        last_status: Some("ok".to_string()),
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "自动清理",
            "entrypoint": "run",
            "scheduleKind": "interval",
            "intervalSeconds": 60,
            "enabled": true
        })
        .to_string(),
        created_at: 1,
        updated_at: 1,
    };
    let manual_task = PluginTask {
        id: "cleanup-banned-accounts::manual".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "手动清理".to_string(),
        description: Some("manual".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "manual".to_string(),
        interval_seconds: None,
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "manual",
            "name": "手动清理",
            "entrypoint": "run",
            "scheduleKind": "manual",
            "enabled": true
        })
        .to_string(),
        created_at: 2,
        updated_at: 2,
    };

    storage
        .replace_plugin_install(&install, &[interval_task, manual_task])
        .expect("seed plugin");

    rearm_enabled_interval_tasks_for_plugin(&storage, Some(&install.plugin_id), 1000)
        .expect("rearm tasks");

    let updated_interval = storage
        .find_plugin_task("cleanup-banned-accounts::run")
        .expect("read interval task")
        .expect("interval task exists");
    assert_eq!(updated_interval.next_run_at, Some(960));
    assert_eq!(updated_interval.last_run_at, Some(900));
    assert_eq!(updated_interval.last_status.as_deref(), Some("ok"));

    let updated_manual = storage
        .find_plugin_task("cleanup-banned-accounts::manual")
        .expect("read manual task")
        .expect("manual task exists");
    assert_eq!(updated_manual.next_run_at, None);
}

#[test]
fn run_loaded_plugin_task_executes_without_refetching_task() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let install = PluginInstall {
        plugin_id: "loaded-task-plugin".to_string(),
        source_url: Some("builtin://codexmanager".to_string()),
        name: "Loaded Task Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: Some("test".to_string()),
        author: Some("CodexManager".to_string()),
        homepage_url: None,
        script_url: None,
        script_body: "fn run(context) { #{ taskId: context.task.id, ok: true } }".to_string(),
        permissions_json: serde_json::json!([]).to_string(),
        manifest_json: serde_json::json!({ "id": "loaded-task-plugin" }).to_string(),
        status: "enabled".to_string(),
        installed_at: 1,
        updated_at: 1,
        last_run_at: None,
        last_error: None,
    };
    let task = PluginTask {
        id: "loaded-task-plugin::run".to_string(),
        plugin_id: install.plugin_id.clone(),
        name: "自动运行".to_string(),
        description: Some("interval".to_string()),
        entrypoint: "run".to_string(),
        schedule_kind: "interval".to_string(),
        interval_seconds: Some(60),
        enabled: true,
        next_run_at: Some(1),
        last_run_at: None,
        last_status: None,
        last_error: None,
        task_json: serde_json::json!({
            "id": "run",
            "name": "自动运行",
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
        .replace_plugin_install(&install, &[task.clone()])
        .expect("seed plugin");

    let loaded_task = PluginTaskExecutionRow {
        id: task.id,
        plugin_id: task.plugin_id,
        name: task.name,
        description: task.description,
        entrypoint: task.entrypoint,
        schedule_kind: task.schedule_kind,
        interval_seconds: task.interval_seconds,
        enabled: task.enabled,
    };
    let output =
        runtime::run_loaded_plugin_task(&storage, loaded_task, None).expect("run loaded task");

    assert_eq!(output["ok"], true);
    assert_eq!(output["taskId"], "loaded-task-plugin::run");

    let updated_task = storage
        .find_plugin_task("loaded-task-plugin::run")
        .expect("read updated task")
        .expect("task exists");
    assert_eq!(updated_task.last_status.as_deref(), Some("ok"));
    assert!(updated_task.last_run_at.is_some());
    assert!(updated_task.next_run_at.is_some());

    let logs = storage
        .list_plugin_run_logs(
            Some("loaded-task-plugin"),
            Some("loaded-task-plugin::run"),
            10,
        )
        .expect("read run logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status, "ok");
    assert_eq!(logs[0].run_type, "scheduled");
}
