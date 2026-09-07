CREATE TABLE IF NOT EXISTS account_agent_identities (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  agent_runtime_id TEXT NOT NULL,
  agent_private_key TEXT NOT NULL,
  task_id TEXT,
  chatgpt_user_id TEXT NOT NULL,
  chatgpt_account_is_fedramp INTEGER NOT NULL DEFAULT 0,
  auth_mode TEXT NOT NULL DEFAULT 'agentIdentity',
  workspace_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
