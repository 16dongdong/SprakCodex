CREATE INDEX IF NOT EXISTS idx_plugin_tasks_due_lookup
  ON plugin_tasks(enabled, next_run_at, created_at, plugin_id)
  WHERE schedule_kind <> 'manual';
