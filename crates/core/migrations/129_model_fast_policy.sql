ALTER TABLE models ADD COLUMN fast_policy TEXT NOT NULL DEFAULT 'passthrough'
  CHECK (fast_policy IN ('passthrough', 'filter', 'force', 'block'));
