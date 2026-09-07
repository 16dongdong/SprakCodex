-- The parameterized capability payloads are copied from the reviewed V2 fixture by
-- Storage::apply_model_catalog_codex_metadata_migration. Keep this SQL marker in the
-- ordered migration set so existing databases receive the same one-time cutover.
INSERT INTO model_catalog_v2_meta(key, value)
VALUES('codex_metadata_revision', '3')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('builtin_revision', '3')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
