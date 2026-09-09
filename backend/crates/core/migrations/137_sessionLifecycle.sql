CREATE TABLE session_routing_bindings_next (
  session_id TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  route_source TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL,
  requested_account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  previous_account_id TEXT,
  last_account_id TEXT,
  reason TEXT NOT NULL DEFAULT 'initial_assignment'
);
INSERT INTO session_routing_bindings_next(session_id,account_id,route_source,status,created_at,updated_at,last_used_at,last_account_id)
SELECT session_id,account_id,route_source,status,created_at,updated_at,last_used_at,account_id FROM session_routing_bindings;
DROP TABLE session_routing_bindings;
ALTER TABLE session_routing_bindings_next RENAME TO session_routing_bindings;
CREATE INDEX idx_session_routing_bindings_account ON session_routing_bindings(account_id,status,last_used_at DESC);
CREATE INDEX idx_session_routing_bindings_last_used ON session_routing_bindings(last_used_at DESC,session_id);
CREATE TABLE session_routing_cooldowns (
 account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
 until_at INTEGER NOT NULL
);
