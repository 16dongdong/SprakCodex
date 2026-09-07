ALTER TABLE request_token_stats
  ADD COLUMN usage_included INTEGER NOT NULL DEFAULT 1
  CHECK (usage_included IN (0, 1));

UPDATE request_token_stats
SET usage_included = CASE
  WHEN EXISTS (
    SELECT 1
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
      AND request_logs.status_code >= 200
      AND request_logs.status_code <= 299
  ) THEN 1
  ELSE 0
END;

UPDATE request_token_stat_hourly_rollups
SET
  input_tokens = 0,
  cached_input_tokens = 0,
  output_tokens = 0,
  total_tokens = 0,
  reasoning_output_tokens = 0,
  estimated_cost_usd = 0.0
WHERE success_count = 0;

CREATE INDEX IF NOT EXISTS idx_request_token_stats_success_key_model_created_at
  ON request_token_stats(key_id, model, created_at DESC)
  WHERE usage_included = 1;
