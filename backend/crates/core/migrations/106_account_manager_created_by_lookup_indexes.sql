CREATE INDEX IF NOT EXISTS idx_app_wallet_ledger_created_by_lookup
  ON app_wallet_ledger_entries(created_by_user_id);

CREATE INDEX IF NOT EXISTS idx_redeem_code_batches_created_by_lookup
  ON redeem_code_batches(created_by_user_id);
