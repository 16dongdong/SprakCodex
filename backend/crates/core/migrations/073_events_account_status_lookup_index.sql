CREATE INDEX IF NOT EXISTS idx_events_account_status_lookup
  ON events(type, account_id, created_at DESC, id DESC);
