CREATE TEMP TABLE IF NOT EXISTS _gpt56_current_price_candidates (
  model_id TEXT PRIMARY KEY
);

DELETE FROM _gpt56_current_price_candidates;

INSERT INTO _gpt56_current_price_candidates(model_id)
SELECT m.id
FROM models m
JOIN model_prices p ON p.model_id = m.id
WHERE m.origin = 'builtin'
  AND m.user_edited = 0
  AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND p.price_status = 'official'
  AND p.price_source = 'https://developers.openai.com/api/docs/models/compare';

DELETE FROM model_price_tiers
WHERE model_id IN (SELECT model_id FROM _gpt56_current_price_candidates);

INSERT INTO model_price_tiers(
  model_id,
  min_input_tokens,
  input_microusd_per_1m,
  cached_input_microusd_per_1m,
  output_microusd_per_1m
)
SELECT
  m.id,
  tier.min_input_tokens,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN CASE tier.min_input_tokens WHEN 0 THEN 5000000 ELSE 10000000 END
    WHEN 'gpt-5.6-terra' THEN CASE tier.min_input_tokens WHEN 0 THEN 2000000 ELSE 4000000 END
    WHEN 'gpt-5.6-luna' THEN CASE tier.min_input_tokens WHEN 0 THEN 200000 ELSE 400000 END
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN CASE tier.min_input_tokens WHEN 0 THEN 500000 ELSE 1000000 END
    WHEN 'gpt-5.6-terra' THEN CASE tier.min_input_tokens WHEN 0 THEN 200000 ELSE 400000 END
    WHEN 'gpt-5.6-luna' THEN CASE tier.min_input_tokens WHEN 0 THEN 20000 ELSE 40000 END
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN CASE tier.min_input_tokens WHEN 0 THEN 30000000 ELSE 45000000 END
    WHEN 'gpt-5.6-terra' THEN CASE tier.min_input_tokens WHEN 0 THEN 12000000 ELSE 18000000 END
    WHEN 'gpt-5.6-luna' THEN CASE tier.min_input_tokens WHEN 0 THEN 1200000 ELSE 1800000 END
  END
FROM models m
JOIN _gpt56_current_price_candidates candidate ON candidate.model_id = m.id
CROSS JOIN (
  SELECT 0 AS min_input_tokens
  UNION ALL
  SELECT 272000
) tier;

UPDATE model_prices
SET input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 5000000
      WHEN 'gpt-5.6-terra' THEN 2000000
      WHEN 'gpt-5.6-luna' THEN 200000
    END,
    cached_input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 500000
      WHEN 'gpt-5.6-terra' THEN 200000
      WHEN 'gpt-5.6-luna' THEN 20000
    END,
    output_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 30000000
      WHEN 'gpt-5.6-terra' THEN 12000000
      WHEN 'gpt-5.6-luna' THEN 1200000
    END,
    price_status = 'official',
    price_source = 'https://developers.openai.com/api/docs/models/compare',
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE model_id IN (SELECT model_id FROM _gpt56_current_price_candidates);

UPDATE models
SET builtin_revision = MAX(COALESCE(builtin_revision, 0), 6),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE id IN (SELECT model_id FROM _gpt56_current_price_candidates);

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt56_pricing_revision', '2026-07-31-official')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('builtin_revision', '6')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
