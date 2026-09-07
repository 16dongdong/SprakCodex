CREATE INDEX IF NOT EXISTS idx_accounts_preferred_updated_at
    ON accounts(preferred, updated_at DESC, id ASC);