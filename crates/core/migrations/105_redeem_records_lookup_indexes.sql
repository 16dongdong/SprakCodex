CREATE INDEX IF NOT EXISTS idx_redeem_records_code_lookup
  ON redeem_records(code_id);

CREATE INDEX IF NOT EXISTS idx_redeem_records_wallet_lookup
  ON redeem_records(wallet_id);

CREATE INDEX IF NOT EXISTS idx_redeem_records_ledger_entry_lookup
  ON redeem_records(ledger_entry_id);
