CREATE INDEX IF NOT EXISTS idx_plugin_installs_list_order
  ON plugin_installs(updated_at DESC, installed_at DESC, plugin_id ASC);
