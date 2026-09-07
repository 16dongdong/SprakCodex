# Minimum access instructions for plugin center

This is the minimum integration guide for quickly connecting a third-party or internal plugin source. As long as your endpoint returns data using the following fields and interfaces, it can integrate with the CodexManager plugin center.

## 1. What do you want to pick up?

- Front-end routing: `/plugins/`
- Market Mode: `builtin` / `private` / `custom`
- Data format: Plugin list JSON
- Execution method: Rhai Script

## 2. Minimum market return

Returns the top-level array, or returns:

```json
{ "items": [] }
```

Each plugin must have at least:

```json
{
  "id": "cleanup-banned-accounts",
  "name": "Clear banned accounts",
  "version": "1.0.0",
  "scriptBody": "fn run(context) { ... }",
  "permissions": ["accounts:cleanup"],
  "tasks": [
    {
      "id": "run",
      "name": "Scheduled automatic cleaning",
      "entrypoint": "run",
      "scheduleKind": "interval",
      "intervalSeconds": 60,
      "enabled": true
    }
  ]
}
```

## 3. Minimum field

| Field | Required | Description |
| --- | --- | --- |
| `id` | Yes | Unique ID |
| `name` | No | Name, default falls back to `id` |
| `version` | No | Version number, default `0.0.0` |
| `scriptBody` | Choose one | Embed script directly |
| `scriptUrl` | Choose one | Script address |
| `permissions` | No | Permission List |
| `tasks` | No | Task Definition |

It is recommended to add:

- `manifestVersion`
- `category`
- `runtimeKind`
- `tags`

## 4. Minimal interface

### Tauri

- `service_plugin_catalog_list`
- `service_plugin_install`
- `service_plugin_update`
- `service_plugin_uninstall`
- `service_plugin_list`
- `service_plugin_enable`
- `service_plugin_disable`
- `service_plugin_tasks_update`
- `service_plugin_tasks_list`
- `service_plugin_tasks_run`
- `service_plugin_logs_list`

### RPC

- `plugin/catalog/list`
- `plugin/install`
- `plugin/update`
- `plugin/uninstall`
- `plugin/list`
- `plugin/enable`
- `plugin/disable`
- `plugin/tasks/update`
- `plugin/tasks/list`
- `plugin/tasks/run`
- `plugin/logs/list`

## 5. Rhai Minimum available function

If the plugin applies for these permissions, it can use the corresponding functions:

- `settings:read` -> `get_setting(key)`, `list_settings()`
- `network` -> `http_get(url)`, `http_post(url, body)`
- `accounts:cleanup` -> `cleanup_banned_accounts()`, `cleanup_unavailable_free_accounts()`

Public functions:

- `log(message)`

## 6. Entrance conclusion

If you are:

- Official selected market, directly returns built-in JSON with the same structure
- Enterprise private marketplace: return the private source JSON
- To customize the repository, just keep `scriptBody` or `scriptUrl`

Currently, the most stable connection method is "list first, script later". Do not pile heavy logic directly into Rhai.