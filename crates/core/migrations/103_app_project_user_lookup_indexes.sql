CREATE INDEX IF NOT EXISTS idx_app_projects_owner_user_lookup
  ON app_projects(owner_user_id, id);

CREATE INDEX IF NOT EXISTS idx_app_project_members_user_lookup
  ON app_project_members(user_id, project_id);
