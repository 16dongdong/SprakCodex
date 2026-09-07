CREATE INDEX IF NOT EXISTS idx_request_logs_status_code_created_at_id
  ON request_logs(status_code, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_method_created_at_id
  ON request_logs(method, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_key_id_created_at_id
  ON request_logs(key_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_account_id_created_at_id
  ON request_logs(account_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_trace_id_created_at_id
  ON request_logs(trace_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_model_created_at_id
  ON request_logs(model, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_request_type_created_at_id
  ON request_logs(request_type, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_gateway_mode_created_at_id
  ON request_logs(gateway_mode, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_route_strategy_created_at_id
  ON request_logs(route_strategy, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_route_source_created_at_id
  ON request_logs(route_source, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_request_logs_actual_source_id_created_at_id
  ON request_logs(actual_source_id, created_at DESC, id DESC);
