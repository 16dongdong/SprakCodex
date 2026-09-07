CREATE TABLE IF NOT EXISTS request_token_stat_hourly_rollups (
  bucket_start INTEGER NOT NULL,
  bucket_end INTEGER NOT NULL,
  key_id TEXT NOT NULL DEFAULT '',
  account_id TEXT NOT NULL DEFAULT '',
  model TEXT NOT NULL DEFAULT '',
  actual_source_kind TEXT NOT NULL DEFAULT '',
  actual_source_id TEXT NOT NULL DEFAULT '',
  owner_user_id TEXT NOT NULL DEFAULT '',
  input_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
  request_count INTEGER NOT NULL DEFAULT 0,
  success_count INTEGER NOT NULL DEFAULT 0,
  error_count INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(bucket_start, key_id, account_id, model, actual_source_kind, actual_source_id, owner_user_id)
);

CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_bucket_start
  ON request_token_stat_hourly_rollups(bucket_start);

CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_key_bucket
  ON request_token_stat_hourly_rollups(key_id, bucket_start);

CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_owner_bucket
  ON request_token_stat_hourly_rollups(owner_user_id, bucket_start);

CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_source_bucket
  ON request_token_stat_hourly_rollups(actual_source_kind, actual_source_id, bucket_start);
