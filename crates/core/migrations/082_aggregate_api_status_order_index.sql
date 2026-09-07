CREATE INDEX IF NOT EXISTS idx_aggregate_apis_status_order
  ON aggregate_apis(
    LOWER(TRIM(COALESCE(status, ''))),
    sort ASC,
    created_at DESC,
    id ASC
  );
