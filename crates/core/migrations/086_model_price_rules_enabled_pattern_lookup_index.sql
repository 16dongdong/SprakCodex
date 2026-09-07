CREATE INDEX IF NOT EXISTS idx_model_price_rules_enabled_pattern_lookup
  ON model_price_rules(enabled, LOWER(TRIM(model_pattern)));
