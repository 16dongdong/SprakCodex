CREATE INDEX IF NOT EXISTS idx_model_source_mappings_source_platform
    ON model_source_mappings(source_kind, source_id, platform_model_slug);

CREATE INDEX IF NOT EXISTS idx_model_source_mappings_kind_enabled_platform
    ON model_source_mappings(source_kind, enabled, platform_model_slug);