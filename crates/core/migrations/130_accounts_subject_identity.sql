ALTER TABLE accounts ADD COLUMN subject_account_id TEXT;

UPDATE accounts
SET subject_account_id = CASE
  WHEN instr(id, '::') > 0 THEN substr(id, 1, instr(id, '::') - 1)
  ELSE id
END
WHERE subject_account_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_accounts_subject_identity
  ON accounts(subject_account_id, updated_at DESC, id ASC);
