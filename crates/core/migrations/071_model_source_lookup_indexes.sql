CREATE INDEX IF NOT EXISTS idx_model_source_models_kind_upstream_status_source
  ON model_source_models(source_kind, upstream_model, status, source_id);

CREATE INDEX IF NOT EXISTS idx_model_source_models_source_status_upstream
  ON model_source_models(source_kind, source_id, status, upstream_model);

CREATE INDEX IF NOT EXISTS idx_model_source_mappings_platform_enabled_priority_weight
  ON model_source_mappings(platform_model_slug, enabled, priority DESC, weight DESC, source_kind, source_id, upstream_model);

CREATE INDEX IF NOT EXISTS idx_model_source_mappings_platform_source_enabled_priority
  ON model_source_mappings(platform_model_slug, source_kind, source_id, enabled, priority DESC, weight DESC, upstream_model);
