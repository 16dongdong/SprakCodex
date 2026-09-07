CREATE INDEX IF NOT EXISTS idx_api_keys_list_order
  ON api_keys(created_at DESC, id ASC);
