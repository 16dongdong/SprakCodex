CREATE INDEX IF NOT EXISTS idx_request_token_stats_model_created_at
  ON request_token_stats(model, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_token_stats_key_model_created_at
  ON request_token_stats(key_id, model, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_token_stats_account_model_created_at
  ON request_token_stats(account_id, model, created_at DESC);
