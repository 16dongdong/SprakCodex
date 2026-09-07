CREATE INDEX IF NOT EXISTS idx_aggregate_apis_list_order
  ON aggregate_apis(sort ASC, updated_at DESC, id ASC);

DROP INDEX IF EXISTS idx_aggregate_apis_created_at;
