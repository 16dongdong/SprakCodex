CREATE TEMP TABLE IF NOT EXISTS _gpt56_official_price_candidates (
  model_id TEXT PRIMARY KEY
);

DELETE FROM _gpt56_official_price_candidates;

INSERT INTO _gpt56_official_price_candidates(model_id)
SELECT m.id
FROM models m
JOIN model_prices p ON p.model_id = m.id
JOIN model_price_tiers base
  ON base.model_id = m.id AND base.min_input_tokens = 0
WHERE m.origin = 'builtin'
  AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND p.price_status = 'estimated'
  AND p.price_source = 'user_provided_openai_gpt-5.6_2026-07-14_cached_at_input_rate'
  AND p.input_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 5000000
    WHEN 'gpt-5.6-terra' THEN 2500000
    WHEN 'gpt-5.6-luna' THEN 1000000
  END
  AND p.cached_input_microusd_per_1m = p.input_microusd_per_1m
  AND p.output_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 30000000
    WHEN 'gpt-5.6-terra' THEN 15000000
    WHEN 'gpt-5.6-luna' THEN 6000000
  END
  AND base.input_microusd_per_1m = p.input_microusd_per_1m
  AND base.cached_input_microusd_per_1m = p.cached_input_microusd_per_1m
  AND base.output_microusd_per_1m = p.output_microusd_per_1m
  AND (
    SELECT COUNT(*)
    FROM model_price_tiers tier
    WHERE tier.model_id = m.id
  ) = 1;

UPDATE model_price_tiers
SET cached_input_microusd_per_1m = CASE lower((
      SELECT slug FROM models WHERE id = model_price_tiers.model_id
    ))
      WHEN 'gpt-5.6-sol' THEN 500000
      WHEN 'gpt-5.6-terra' THEN 250000
      WHEN 'gpt-5.6-luna' THEN 100000
    END
WHERE min_input_tokens = 0
  AND model_id IN (SELECT model_id FROM _gpt56_official_price_candidates);

INSERT INTO model_price_tiers(
  model_id,
  min_input_tokens,
  input_microusd_per_1m,
  cached_input_microusd_per_1m,
  output_microusd_per_1m
)
SELECT
  m.id,
  272000,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 10000000
    WHEN 'gpt-5.6-terra' THEN 5000000
    WHEN 'gpt-5.6-luna' THEN 2000000
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 1000000
    WHEN 'gpt-5.6-terra' THEN 500000
    WHEN 'gpt-5.6-luna' THEN 200000
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 45000000
    WHEN 'gpt-5.6-terra' THEN 22500000
    WHEN 'gpt-5.6-luna' THEN 9000000
  END
FROM models m
JOIN _gpt56_official_price_candidates candidate ON candidate.model_id = m.id;

UPDATE model_prices
SET cached_input_microusd_per_1m = CASE lower((
      SELECT slug FROM models WHERE id = model_prices.model_id
    ))
      WHEN 'gpt-5.6-sol' THEN 500000
      WHEN 'gpt-5.6-terra' THEN 250000
      WHEN 'gpt-5.6-luna' THEN 100000
    END,
    price_status = 'official',
    price_source = 'https://developers.openai.com/api/docs/models/compare',
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE model_id IN (SELECT model_id FROM _gpt56_official_price_candidates);

UPDATE models
SET builtin_revision = MAX(COALESCE(builtin_revision, 0), 4),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE id IN (SELECT model_id FROM _gpt56_official_price_candidates);

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt56_pricing_revision', '2026-07-20-official')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
