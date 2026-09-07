CREATE INDEX IF NOT EXISTS idx_user_model_groups_group_lookup
  ON user_model_groups(group_id, user_id);
