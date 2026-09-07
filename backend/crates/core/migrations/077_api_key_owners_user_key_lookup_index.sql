CREATE INDEX IF NOT EXISTS idx_api_key_owners_user_key_lookup
  ON api_key_owners(owner_user_id, key_id)
  WHERE owner_kind = 'user';
