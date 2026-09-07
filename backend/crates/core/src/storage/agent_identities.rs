use rusqlite::{OptionalExtension, Result};

use super::{now_ts, AccountAgentIdentity, Storage};

pub(super) fn delete_account_agent_identity_for_account_sql() -> &'static str {
    "DELETE FROM account_agent_identities WHERE account_id = ?1"
}

impl Storage {
    pub fn upsert_account_agent_identity(&self, identity: &AccountAgentIdentity) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO account_agent_identities (
                account_id,
                agent_runtime_id,
                agent_private_key,
                task_id,
                chatgpt_user_id,
                chatgpt_account_is_fedramp,
                auth_mode,
                workspace_id,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(account_id) DO UPDATE SET
                agent_runtime_id = excluded.agent_runtime_id,
                agent_private_key = excluded.agent_private_key,
                task_id = excluded.task_id,
                chatgpt_user_id = excluded.chatgpt_user_id,
                chatgpt_account_is_fedramp = excluded.chatgpt_account_is_fedramp,
                auth_mode = excluded.auth_mode,
                workspace_id = excluded.workspace_id,
                updated_at = excluded.updated_at",
            (
                &identity.account_id,
                &identity.agent_runtime_id,
                &identity.agent_private_key,
                &identity.task_id,
                &identity.chatgpt_user_id,
                if identity.chatgpt_account_is_fedramp {
                    1
                } else {
                    0
                },
                &identity.auth_mode,
                &identity.workspace_id,
                identity.created_at,
                now,
            ),
        )?;
        Ok(())
    }

    pub fn find_account_agent_identity(
        &self,
        account_id: &str,
    ) -> Result<Option<AccountAgentIdentity>> {
        self.conn
            .query_row(
                "SELECT
                    account_id,
                    agent_runtime_id,
                    agent_private_key,
                    task_id,
                    chatgpt_user_id,
                    chatgpt_account_is_fedramp,
                    auth_mode,
                    workspace_id,
                    created_at,
                    updated_at
                 FROM account_agent_identities
                 WHERE account_id = ?1
                   AND LOWER(TRIM(auth_mode)) = 'agentidentity'
                 LIMIT 1",
                [account_id],
                |row| {
                    Ok(AccountAgentIdentity {
                        account_id: row.get(0)?,
                        agent_runtime_id: row.get(1)?,
                        agent_private_key: row.get(2)?,
                        task_id: row.get(3)?,
                        chatgpt_user_id: row.get(4)?,
                        chatgpt_account_is_fedramp: row.get::<_, i64>(5)? != 0,
                        auth_mode: row.get(6)?,
                        workspace_id: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .optional()
    }

    pub fn update_account_agent_identity_task_id(
        &self,
        account_id: &str,
        expected_agent_runtime_id: &str,
        expected_agent_private_key: &str,
        task_id: Option<&str>,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE account_agent_identities
             SET task_id = ?1, updated_at = ?2
             WHERE account_id = ?3
               AND agent_runtime_id = ?4
               AND agent_private_key = ?5
               AND LOWER(TRIM(auth_mode)) = 'agentidentity'",
            (
                task_id,
                now_ts(),
                account_id,
                expected_agent_runtime_id,
                expected_agent_private_key,
            ),
        )?;
        Ok(updated > 0)
    }

    pub fn delete_account_agent_identity(&self, account_id: &str) -> Result<()> {
        self.conn.execute(
            delete_account_agent_identity_for_account_sql(),
            [account_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "agent_identities_tests.rs"]
mod tests;
