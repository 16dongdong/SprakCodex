CREATE TABLE IF NOT EXISTS account_routing_preferences (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  enabled INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session_routing_bindings (
  session_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  route_source TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_routing_bindings_account
  ON session_routing_bindings(account_id, status, last_used_at DESC);

CREATE INDEX IF NOT EXISTS idx_session_routing_bindings_last_used
  ON session_routing_bindings(status, last_used_at DESC);
