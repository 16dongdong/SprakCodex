CREATE TABLE models (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name TEXT NOT NULL,
  description TEXT,
  provider TEXT,
  family TEXT,
  category TEXT,
  tags_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tags_json)),
  origin TEXT NOT NULL CHECK (origin IN ('builtin', 'custom')),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  supported_in_api INTEGER NOT NULL DEFAULT 1 CHECK (supported_in_api IN (0, 1)),
  visibility TEXT NOT NULL DEFAULT 'list' CHECK (visibility IN ('list', 'hide')),
  sort_order INTEGER NOT NULL DEFAULT 0,
  context_window INTEGER CHECK (context_window IS NULL OR context_window > 0),
  max_context_window INTEGER CHECK (max_context_window IS NULL OR max_context_window > 0),
  default_reasoning_effort TEXT,
  capabilities_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(capabilities_json)),
  instructions_mode TEXT NOT NULL DEFAULT 'passthrough'
    CHECK (instructions_mode IN ('passthrough', 'fallback', 'override')),
  instructions_text TEXT,
  builtin_revision INTEGER CHECK (builtin_revision IS NULL OR builtin_revision > 0),
  user_edited INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0, 1)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK (origin = 'custom' OR builtin_revision IS NOT NULL),
  CHECK (instructions_mode <> 'override' OR length(trim(instructions_text)) > 0)
);

CREATE INDEX idx_models_enabled_visibility_order
  ON models(enabled, supported_in_api, visibility, sort_order, slug);
CREATE INDEX idx_models_origin_order ON models(origin, sort_order, slug);

CREATE TABLE model_prices (
  model_id TEXT PRIMARY KEY REFERENCES models(id) ON DELETE CASCADE,
  currency TEXT NOT NULL DEFAULT 'USD' CHECK (currency = 'USD'),
  input_microusd_per_1m INTEGER,
  cached_input_microusd_per_1m INTEGER,
  output_microusd_per_1m INTEGER,
  price_status TEXT NOT NULL
    CHECK (price_status IN ('official', 'estimated', 'custom', 'missing')),
  price_source TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK (input_microusd_per_1m IS NULL OR input_microusd_per_1m >= 0),
  CHECK (cached_input_microusd_per_1m IS NULL OR cached_input_microusd_per_1m >= 0),
  CHECK (output_microusd_per_1m IS NULL OR output_microusd_per_1m >= 0),
  CHECK (
    (price_status = 'missing'
      AND input_microusd_per_1m IS NULL
      AND cached_input_microusd_per_1m IS NULL
      AND output_microusd_per_1m IS NULL)
    OR
    (price_status <> 'missing'
      AND input_microusd_per_1m IS NOT NULL
      AND cached_input_microusd_per_1m IS NOT NULL
      AND output_microusd_per_1m IS NOT NULL)
  )
);

CREATE TABLE model_price_tiers (
  model_id TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  min_input_tokens INTEGER NOT NULL CHECK (min_input_tokens >= 0),
  input_microusd_per_1m INTEGER NOT NULL CHECK (input_microusd_per_1m >= 0),
  cached_input_microusd_per_1m INTEGER NOT NULL CHECK (cached_input_microusd_per_1m >= 0),
  output_microusd_per_1m INTEGER NOT NULL CHECK (output_microusd_per_1m >= 0),
  PRIMARY KEY (model_id, min_input_tokens)
);

CREATE INDEX idx_model_price_tiers_lookup
  ON model_price_tiers(model_id, min_input_tokens DESC);

CREATE TABLE model_routes (
  id TEXT PRIMARY KEY,
  model_id TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL CHECK (source_kind IN ('account_pool', 'aggregate_api')),
  source_id TEXT NOT NULL CHECK (length(trim(source_id)) > 0),
  upstream_model TEXT NOT NULL CHECK (length(trim(upstream_model)) > 0),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  priority INTEGER NOT NULL DEFAULT 0,
  weight INTEGER NOT NULL DEFAULT 1 CHECK (weight > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(model_id, source_kind, source_id, upstream_model)
);

CREATE INDEX idx_model_routes_model_enabled_order
  ON model_routes(model_id, enabled, priority DESC, id);
CREATE INDEX idx_model_routes_source
  ON model_routes(source_kind, source_id, enabled);

CREATE TABLE model_group_models_v2 (
  group_id TEXT NOT NULL REFERENCES model_groups(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  rate_multiplier_millis INTEGER CHECK (rate_multiplier_millis IS NULL OR rate_multiplier_millis > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (group_id, model_id)
);

CREATE INDEX idx_model_group_models_v2_model
  ON model_group_models_v2(model_id, enabled, group_id);

CREATE TABLE request_charge_snapshots (
  request_log_id INTEGER PRIMARY KEY REFERENCES request_logs(id) ON DELETE RESTRICT,
  model_id TEXT REFERENCES models(id) ON DELETE SET NULL,
  model_slug TEXT NOT NULL,
  tier_min_input_tokens INTEGER NOT NULL CHECK (tier_min_input_tokens >= 0),
  usage_source TEXT NOT NULL CHECK (usage_source IN ('actual', 'estimated')),
  input_tokens INTEGER NOT NULL CHECK (input_tokens >= 0),
  cached_input_tokens INTEGER NOT NULL CHECK (cached_input_tokens >= 0),
  output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
  input_microusd_per_1m INTEGER NOT NULL CHECK (input_microusd_per_1m >= 0),
  cached_input_microusd_per_1m INTEGER NOT NULL CHECK (cached_input_microusd_per_1m >= 0),
  output_microusd_per_1m INTEGER NOT NULL CHECK (output_microusd_per_1m >= 0),
  rate_multiplier_millis INTEGER NOT NULL CHECK (rate_multiplier_millis > 0),
  base_cost_microusd INTEGER NOT NULL CHECK (base_cost_microusd >= 0),
  charged_cost_microusd INTEGER NOT NULL CHECK (charged_cost_microusd >= 0),
  currency TEXT NOT NULL DEFAULT 'USD' CHECK (currency = 'USD'),
  created_at INTEGER NOT NULL
);

CREATE INDEX idx_request_charge_snapshots_model_created
  ON request_charge_snapshots(model_slug, created_at DESC);

CREATE TABLE model_catalog_v2_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
