CREATE INDEX IF NOT EXISTS idx_tokens_refresh_due_order
  ON tokens(COALESCE(next_refresh_at, 0) ASC, account_id ASC);
