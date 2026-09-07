DELETE FROM model_price_tiers
WHERE model_id IN (
  SELECT m.id
  FROM models m
  JOIN model_prices p ON p.model_id = m.id
  WHERE m.origin = 'builtin'
    AND m.user_edited = 0
    AND p.price_status = 'missing'
    AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
);

INSERT INTO model_price_tiers(
  model_id,
  min_input_tokens,
  input_microusd_per_1m,
  cached_input_microusd_per_1m,
  output_microusd_per_1m
)
SELECT
  m.id,
  0,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 5000000
    WHEN 'gpt-5.6-terra' THEN 2500000
    WHEN 'gpt-5.6-luna' THEN 1000000
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 5000000
    WHEN 'gpt-5.6-terra' THEN 2500000
    WHEN 'gpt-5.6-luna' THEN 1000000
  END,
  CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 30000000
    WHEN 'gpt-5.6-terra' THEN 15000000
    WHEN 'gpt-5.6-luna' THEN 6000000
  END
FROM models m
JOIN model_prices p ON p.model_id = m.id
WHERE m.origin = 'builtin'
  AND m.user_edited = 0
  AND p.price_status = 'missing'
  AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna');

UPDATE model_prices
SET input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 5000000
      WHEN 'gpt-5.6-terra' THEN 2500000
      WHEN 'gpt-5.6-luna' THEN 1000000
    END,
    cached_input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 5000000
      WHEN 'gpt-5.6-terra' THEN 2500000
      WHEN 'gpt-5.6-luna' THEN 1000000
    END,
    output_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 30000000
      WHEN 'gpt-5.6-terra' THEN 15000000
      WHEN 'gpt-5.6-luna' THEN 6000000
    END,
    price_status = 'estimated',
    price_source = 'user_provided_openai_gpt-5.6_2026-07-14_cached_at_input_rate',
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE price_status = 'missing'
  AND model_id IN (
    SELECT id
    FROM models
    WHERE origin = 'builtin'
      AND user_edited = 0
      AND lower(slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  );

UPDATE models
SET builtin_revision = MAX(COALESCE(builtin_revision, 0), 2),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE origin = 'builtin'
  AND user_edited = 0
  AND lower(slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND id IN (
    SELECT model_id
    FROM model_prices
    WHERE price_source = 'user_provided_openai_gpt-5.6_2026-07-14_cached_at_input_rate'
  );

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt56_pricing_revision', '2026-07-14')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
