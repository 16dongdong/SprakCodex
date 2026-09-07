CREATE INDEX IF NOT EXISTS idx_aggregate_apis_balance_query_order
  ON aggregate_apis(
    balance_query_enabled,
    sort ASC,
    updated_at DESC,
    id ASC
  );
