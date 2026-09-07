ALTER TABLE model_prices
  ADD COLUMN cache_write_microusd_per_1m INTEGER
  CHECK(cache_write_microusd_per_1m IS NULL OR cache_write_microusd_per_1m >= 0);

ALTER TABLE model_price_tiers
  ADD COLUMN cache_write_microusd_per_1m INTEGER
  CHECK(cache_write_microusd_per_1m IS NULL OR cache_write_microusd_per_1m >= 0);

ALTER TABLE request_charge_snapshots
  ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0
  CHECK(cache_write_tokens >= 0);

ALTER TABLE request_charge_snapshots
  ADD COLUMN cache_write_microusd_per_1m INTEGER NOT NULL DEFAULT 0
  CHECK(cache_write_microusd_per_1m >= 0);

UPDATE request_charge_snapshots
SET cache_write_microusd_per_1m = input_microusd_per_1m;

CREATE TEMP TABLE IF NOT EXISTS _gpt56_cache_write_price_candidates (
  model_id TEXT PRIMARY KEY
);

DELETE FROM _gpt56_cache_write_price_candidates;

INSERT INTO _gpt56_cache_write_price_candidates(model_id)
SELECT m.id
FROM models m
JOIN model_prices p ON p.model_id = m.id
WHERE m.origin = 'builtin'
  AND m.user_edited = 0
  AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND p.price_status = 'official'
  AND p.price_source = 'https://developers.openai.com/api/docs/models/compare';

DELETE FROM model_price_tiers
WHERE min_input_tokens = 272000
  AND model_id IN (SELECT model_id FROM _gpt56_cache_write_price_candidates)
  AND EXISTS (
    SELECT 1
    FROM model_price_tiers corrected
    WHERE corrected.model_id = model_price_tiers.model_id
      AND corrected.min_input_tokens = 272001
  );

UPDATE model_price_tiers
SET min_input_tokens = 272001
WHERE min_input_tokens = 272000
  AND model_id IN (SELECT model_id FROM _gpt56_cache_write_price_candidates);

UPDATE model_price_tiers
SET cache_write_microusd_per_1m = CASE lower((
      SELECT slug FROM models WHERE id = model_price_tiers.model_id
    ))
      WHEN 'gpt-5.6-sol' THEN CASE min_input_tokens WHEN 0 THEN 6250000 ELSE 12500000 END
      WHEN 'gpt-5.6-terra' THEN CASE min_input_tokens WHEN 0 THEN 2500000 ELSE 5000000 END
      WHEN 'gpt-5.6-luna' THEN CASE min_input_tokens WHEN 0 THEN 250000 ELSE 500000 END
    END
WHERE model_id IN (SELECT model_id FROM _gpt56_cache_write_price_candidates);

UPDATE model_prices
SET cache_write_microusd_per_1m = CASE lower((
      SELECT slug FROM models WHERE id = model_prices.model_id
    ))
      WHEN 'gpt-5.6-sol' THEN 6250000
      WHEN 'gpt-5.6-terra' THEN 2500000
      WHEN 'gpt-5.6-luna' THEN 250000
    END,
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE model_id IN (SELECT model_id FROM _gpt56_cache_write_price_candidates);

UPDATE models
SET builtin_revision = MAX(COALESCE(builtin_revision, 0), 7),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE id IN (SELECT model_id FROM _gpt56_cache_write_price_candidates);

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt56_cache_write_pricing_revision', '2026-07-31-official')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('builtin_revision', '7')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

DROP TABLE _gpt56_cache_write_price_candidates;
