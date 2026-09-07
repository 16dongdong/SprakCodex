CREATE INDEX IF NOT EXISTS idx_accounts_cleanup_status_lookup
  ON accounts(LOWER(TRIM(COALESCE(status, ''))), sort ASC, updated_at DESC, id ASC);
