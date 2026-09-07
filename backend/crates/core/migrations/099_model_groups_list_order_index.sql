CREATE INDEX IF NOT EXISTS idx_model_groups_list_order
  ON model_groups(sort ASC, name ASC, created_at ASC, id ASC);
