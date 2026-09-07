use super::*;
use crate::storage::Account;

fn insert_account(storage: &Storage, account_id: &str) {
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: account_id.to_string(),
            label: account_id.to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: Some("chatgpt-account".to_string()),
            workspace_id: Some("workspace".to_string()),
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
}

#[test]
fn account_agent_identity_round_trips_and_updates() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_account(&storage, "account-1");
    let now = now_ts();
    let mut identity = AccountAgentIdentity {
        account_id: "account-1".to_string(),
        agent_runtime_id: "agent-runtime-1".to_string(),
        agent_private_key: "private-key-1".to_string(),
        task_id: None,
        chatgpt_user_id: "user-1".to_string(),
        chatgpt_account_is_fedramp: false,
        auth_mode: "agentIdentity".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        created_at: now,
        updated_at: now,
    };

    storage
        .upsert_account_agent_identity(&identity)
        .expect("insert identity");
    let stored = storage
        .find_account_agent_identity("account-1")
        .expect("find identity")
        .expect("stored identity");
    assert_eq!(stored.agent_runtime_id, "agent-runtime-1");
    assert_eq!(stored.task_id, None);
    assert!(!stored.chatgpt_account_is_fedramp);

    identity.task_id = Some("task-2".to_string());
    identity.chatgpt_account_is_fedramp = true;
    storage
        .upsert_account_agent_identity(&identity)
        .expect("update identity");
    let updated = storage
        .find_account_agent_identity("account-1")
        .expect("find updated identity")
        .expect("updated identity");
    assert_eq!(updated.task_id.as_deref(), Some("task-2"));
    assert!(updated.chatgpt_account_is_fedramp);
}

#[test]
fn account_agent_identity_task_id_can_be_registered_and_cleared() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_account(&storage, "account-task-lifecycle");
    let now = now_ts();
    storage
        .upsert_account_agent_identity(&AccountAgentIdentity {
            account_id: "account-task-lifecycle".to_string(),
            agent_runtime_id: "agent-runtime-1".to_string(),
            agent_private_key: "private-key-1".to_string(),
            task_id: None,
            chatgpt_user_id: "user-1".to_string(),
            chatgpt_account_is_fedramp: false,
            auth_mode: "agentIdentity".to_string(),
            workspace_id: None,
            created_at: now,
            updated_at: now,
        })
        .expect("insert identity without task");

    assert!(!storage
        .update_account_agent_identity_task_id(
            "account-task-lifecycle",
            "different-runtime",
            "private-key-1",
            Some("task-stale-registration"),
        )
        .expect("reject stale identity update"));
    assert!(storage
        .update_account_agent_identity_task_id(
            "account-task-lifecycle",
            "agent-runtime-1",
            "private-key-1",
            Some("task-registered"),
        )
        .expect("register task"));
    assert_eq!(
        storage
            .find_account_agent_identity("account-task-lifecycle")
            .expect("find identity")
            .expect("identity")
            .task_id
            .as_deref(),
        Some("task-registered")
    );

    assert!(storage
        .update_account_agent_identity_task_id(
            "account-task-lifecycle",
            "agent-runtime-1",
            "private-key-1",
            None,
        )
        .expect("clear task"));
    assert_eq!(
        storage
            .find_account_agent_identity("account-task-lifecycle")
            .expect("find identity")
            .expect("identity")
            .task_id,
        None
    );
    assert!(!storage
        .update_account_agent_identity_task_id(
            "missing-account",
            "agent-runtime-1",
            "private-key-1",
            Some("task"),
        )
        .expect("missing identity update"));
}

#[test]
fn account_agent_identity_lookup_ignores_non_agent_auth_mode() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_account(&storage, "account-wrong-auth-mode");
    let now = now_ts();
    let mut identity = AccountAgentIdentity {
        account_id: "account-wrong-auth-mode".to_string(),
        agent_runtime_id: "agent-runtime-1".to_string(),
        agent_private_key: "private-key-1".to_string(),
        task_id: None,
        chatgpt_user_id: "user-1".to_string(),
        chatgpt_account_is_fedramp: false,
        auth_mode: "oauth".to_string(),
        workspace_id: None,
        created_at: now,
        updated_at: now,
    };
    storage
        .upsert_account_agent_identity(&identity)
        .expect("insert non-agent identity row");
    assert!(storage
        .find_account_agent_identity(&identity.account_id)
        .expect("lookup non-agent identity")
        .is_none());
    assert!(!storage
        .update_account_agent_identity_task_id(
            &identity.account_id,
            &identity.agent_runtime_id,
            &identity.agent_private_key,
            Some("task-ignored"),
        )
        .expect("ignore task update for non-agent row"));

    identity.auth_mode = " AgentIdentity ".to_string();
    storage
        .upsert_account_agent_identity(&identity)
        .expect("update agent identity mode");
    assert!(storage
        .find_account_agent_identity(&identity.account_id)
        .expect("lookup agent identity")
        .is_some());
}

#[test]
fn deleting_account_cascades_agent_identity() {
    let mut storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_account(&storage, "account-1");
    let now = now_ts();
    storage
        .upsert_account_agent_identity(&AccountAgentIdentity {
            account_id: "account-1".to_string(),
            agent_runtime_id: "agent-runtime-1".to_string(),
            agent_private_key: "private-key-1".to_string(),
            task_id: Some("task-1".to_string()),
            chatgpt_user_id: "user-1".to_string(),
            chatgpt_account_is_fedramp: false,
            auth_mode: "agentIdentity".to_string(),
            workspace_id: None,
            created_at: now,
            updated_at: now,
        })
        .expect("insert identity");

    storage.delete_account("account-1").expect("delete account");
    assert!(storage
        .find_account_agent_identity("account-1")
        .expect("find identity")
        .is_none());
}
