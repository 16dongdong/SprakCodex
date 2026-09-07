CREATE INDEX IF NOT EXISTS idx_app_users_lower_username
  ON app_users(lower(username));
