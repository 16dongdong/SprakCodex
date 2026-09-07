CREATE INDEX IF NOT EXISTS idx_model_catalog_models_scope_order
  ON model_catalog_models(scope, sort_index, updated_at DESC, slug);

DROP INDEX IF EXISTS idx_model_catalog_models_scope_sort;
