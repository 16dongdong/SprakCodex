CREATE TABLE IF NOT EXISTS codex_skill_repositories (
  id TEXT PRIMARY KEY,
  owner TEXT NOT NULL COLLATE NOCASE,
  repository TEXT NOT NULL COLLATE NOCASE,
  ref_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  is_builtin INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
  revision TEXT,
  last_scanned_at INTEGER,
  last_error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK (length(trim(id)) BETWEEN 1 AND 128),
  CHECK (length(trim(owner)) BETWEEN 1 AND 255),
  CHECK (length(trim(repository)) BETWEEN 1 AND 255),
  CHECK (length(trim(ref_name)) <= 255),
  UNIQUE (owner, repository, ref_name)
);

CREATE INDEX IF NOT EXISTS idx_codex_skill_repositories_enabled_order
  ON codex_skill_repositories(enabled DESC, is_builtin DESC, owner, repository, ref_name, id);

CREATE TABLE IF NOT EXISTS codex_skill_repository_skills (
  repository_id TEXT NOT NULL REFERENCES codex_skill_repositories(id) ON DELETE CASCADE,
  skill_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  path TEXT NOT NULL COLLATE NOCASE,
  source_url TEXT NOT NULL,
  revision TEXT,
  PRIMARY KEY (repository_id, skill_id),
  UNIQUE (repository_id, path),
  CHECK (length(trim(skill_id)) BETWEEN 1 AND 256),
  CHECK (length(trim(name)) BETWEEN 1 AND 256),
  CHECK (length(path) BETWEEN 1 AND 2048),
  CHECK (length(source_url) BETWEEN 1 AND 4096)
);

CREATE INDEX IF NOT EXISTS idx_codex_skill_repository_skills_name
  ON codex_skill_repository_skills(repository_id, name, path, skill_id);

INSERT OR IGNORE INTO codex_skill_repositories (
  id, owner, repository, ref_name, enabled, is_builtin,
  revision, last_scanned_at, last_error, created_at, updated_at
)
VALUES
  (
    'builtin-anthropics-skills', 'anthropics', 'skills', 'main', 1, 1,
    NULL, NULL, NULL, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)
  ),
  (
    'builtin-composiohq-awesome-claude-skills', 'ComposioHQ', 'awesome-claude-skills', 'master', 1, 1,
    NULL, NULL, NULL, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)
  ),
  (
    'builtin-cexll-myclaude', 'cexll', 'myclaude', 'master', 1, 1,
    NULL, NULL, NULL, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)
  ),
  (
    'builtin-jimliu-baoyu-skills', 'JimLiu', 'baoyu-skills', 'main', 1, 1,
    NULL, NULL, NULL, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)
  );
