CREATE TABLE IF NOT EXISTS quota_reset_warmup (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    window_minutes INTEGER NOT NULL,
    reset_at INTEGER NOT NULL,
    claimed_at INTEGER,
    PRIMARY KEY (account_id, window_minutes)
);
