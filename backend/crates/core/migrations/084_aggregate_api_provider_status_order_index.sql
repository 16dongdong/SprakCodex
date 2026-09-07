CREATE INDEX IF NOT EXISTS idx_aggregate_apis_provider_status_order
  ON aggregate_apis(
    LOWER(TRIM(COALESCE(status, ''))),
    REPLACE(LOWER(TRIM(COALESCE(provider_type, ''))), '-', '_'),
    sort ASC,
    created_at DESC,
    id ASC
  );
