-- Data rows are inserted by Storage::apply_model_catalog_gpt6_astra_migration
-- from the versioned built-in fixture so fresh and upgraded catalogs share one source.
INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt6_astra_catalog_revision', '2026-09-04-official')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt6_astra_price_source', 'https://developers.openai.com/api/docs/models/gpt-6-astra')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
