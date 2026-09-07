ALTER TABLE model_group_models_v2 RENAME TO model_group_models_v2_112;

CREATE TABLE model_group_models_v2 (
  group_id TEXT NOT NULL REFERENCES model_groups(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  rate_multiplier_millis INTEGER CHECK (
    rate_multiplier_millis IS NULL OR rate_multiplier_millis >= 0
  ),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (group_id, model_id)
);

INSERT INTO model_group_models_v2(
  group_id, model_id, enabled, rate_multiplier_millis, created_at, updated_at
)
SELECT
  group_id, model_id, enabled, rate_multiplier_millis, created_at, updated_at
FROM model_group_models_v2_112;

DROP TABLE model_group_models_v2_112;

CREATE INDEX idx_model_group_models_v2_model
  ON model_group_models_v2(model_id, enabled, group_id);

ALTER TABLE request_charge_snapshots RENAME TO request_charge_snapshots_112;

CREATE TABLE request_charge_snapshots (
  request_log_id INTEGER PRIMARY KEY REFERENCES request_logs(id) ON DELETE RESTRICT,
  model_id TEXT REFERENCES models(id) ON DELETE SET NULL,
  model_slug TEXT NOT NULL,
  tier_min_input_tokens INTEGER NOT NULL CHECK (tier_min_input_tokens >= 0),
  usage_source TEXT NOT NULL CHECK (usage_source IN ('actual', 'estimated')),
  input_tokens INTEGER NOT NULL CHECK (input_tokens >= 0),
  cached_input_tokens INTEGER NOT NULL CHECK (
    cached_input_tokens >= 0 AND cached_input_tokens <= input_tokens
  ),
  output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
  input_microusd_per_1m INTEGER NOT NULL CHECK (input_microusd_per_1m >= 0),
  cached_input_microusd_per_1m INTEGER NOT NULL CHECK (
    cached_input_microusd_per_1m >= 0
  ),
  output_microusd_per_1m INTEGER NOT NULL CHECK (output_microusd_per_1m >= 0),
  rate_multiplier_millis INTEGER NOT NULL CHECK (rate_multiplier_millis >= 0),
  base_cost_microusd INTEGER NOT NULL CHECK (base_cost_microusd >= 0),
  charged_cost_microusd INTEGER NOT NULL CHECK (charged_cost_microusd >= 0),
  currency TEXT NOT NULL DEFAULT 'USD' CHECK (currency = 'USD'),
  created_at INTEGER NOT NULL
);

INSERT INTO request_charge_snapshots(
  request_log_id, model_id, model_slug, tier_min_input_tokens, usage_source,
  input_tokens, cached_input_tokens, output_tokens, input_microusd_per_1m,
  cached_input_microusd_per_1m, output_microusd_per_1m,
  rate_multiplier_millis, base_cost_microusd, charged_cost_microusd,
  currency, created_at
)
SELECT
  request_log_id, model_id, model_slug, tier_min_input_tokens, usage_source,
  input_tokens, MIN(cached_input_tokens, input_tokens), output_tokens,
  input_microusd_per_1m, cached_input_microusd_per_1m,
  output_microusd_per_1m, rate_multiplier_millis, base_cost_microusd,
  charged_cost_microusd, currency, created_at
FROM request_charge_snapshots_112;

DROP TABLE request_charge_snapshots_112;

CREATE INDEX idx_request_charge_snapshots_model_created
  ON request_charge_snapshots(model_slug, created_at DESC);

CREATE UNIQUE INDEX idx_app_wallet_ledger_one_request_charge
  ON app_wallet_ledger_entries(request_log_id)
  WHERE request_log_id IS NOT NULL AND entry_kind = 'request_charge';
