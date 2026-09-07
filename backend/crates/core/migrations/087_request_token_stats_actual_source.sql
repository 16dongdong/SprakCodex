UPDATE request_token_stats
SET
  actual_source_kind = (
    SELECT request_logs.actual_source_kind
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
  ),
  actual_source_id = (
    SELECT request_logs.actual_source_id
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
  )
WHERE request_log_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
      AND (
        request_logs.actual_source_kind IS NOT NULL
        OR request_logs.actual_source_id IS NOT NULL
      )
  );

CREATE INDEX IF NOT EXISTS idx_request_token_stats_actual_source_created_at
  ON request_token_stats(actual_source_kind, actual_source_id, created_at DESC);
