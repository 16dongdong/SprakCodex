CREATE INDEX IF NOT EXISTS idx_request_logs_model_created_at
  ON request_logs(model, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_request_type_created_at
  ON request_logs(request_type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_gateway_mode_created_at
  ON request_logs(gateway_mode, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_route_strategy_created_at
  ON request_logs(route_strategy, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_route_source_created_at
  ON request_logs(route_source, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_actual_source_id_created_at
  ON request_logs(actual_source_id, created_at DESC);
