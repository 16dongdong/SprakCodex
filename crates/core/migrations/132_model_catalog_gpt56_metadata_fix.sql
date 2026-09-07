-- Correct the bundled GPT-5.6 metadata for databases that already applied the
-- earlier catalog migrations. Keep user-edited and future built-in revisions
-- untouched, and avoid changing updated_at when a row is already correct.
UPDATE models
SET context_window = 272000,
    max_context_window = 872000,
    capabilities_json = json_set(capabilities_json, '$.shell_type', 'unified_exec'),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE lower(slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND origin = 'builtin'
  AND user_edited = 0
  AND COALESCE(builtin_revision, 0) <= 8
  AND (
    context_window IS NULL
    OR context_window <> 272000
    OR max_context_window IS NULL
    OR max_context_window <> 872000
    OR COALESCE(json_extract(capabilities_json, '$.shell_type'), '') <> 'unified_exec'
  );
