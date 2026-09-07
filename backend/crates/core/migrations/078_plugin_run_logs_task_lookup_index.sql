CREATE INDEX IF NOT EXISTS idx_plugin_run_logs_task_lookup
  ON plugin_run_logs(task_id, started_at DESC, id DESC);
