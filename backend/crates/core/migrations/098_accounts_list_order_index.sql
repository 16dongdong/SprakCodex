CREATE INDEX IF NOT EXISTS idx_accounts_list_order
  ON accounts(sort ASC, updated_at DESC, id ASC);
