CREATE INDEX IF NOT EXISTS idx_aggregate_apis_balance_query_lookup
  ON aggregate_apis(
    balance_query_enabled,
    LOWER(TRIM(COALESCE(status, ''))),
    sort ASC,
    updated_at DESC,
    id ASC
  );
