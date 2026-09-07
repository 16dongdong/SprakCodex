CREATE INDEX IF NOT EXISTS idx_accounts_chatgpt_account_id_updated_at
  ON accounts(chatgpt_account_id, updated_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_accounts_workspace_id_updated_at
  ON accounts(workspace_id, updated_at DESC, id ASC);
