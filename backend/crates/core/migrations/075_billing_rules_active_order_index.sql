CREATE INDEX IF NOT EXISTS idx_billing_rules_active_order
  ON billing_rules(status, priority DESC, updated_at DESC, name ASC);
