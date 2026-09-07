CREATE INDEX IF NOT EXISTS idx_model_price_rules_custom_exact_lookup
  ON model_price_rules(
    source,
    enabled,
    match_type COLLATE NOCASE,
    model_pattern COLLATE NOCASE,
    priority DESC,
    id ASC
  );
