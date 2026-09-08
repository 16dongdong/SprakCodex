use super::*;
use crate::storage::{now_ts, Account, Token};

/// 创建具备真实账号、令牌和迁移结构的内存库；测试失败时直接返回数据库错误。
fn storage() -> Storage {
    let storage = Storage::open_in_memory().expect("打开内存数据库");
    storage.init().expect("初始化数据库");
    storage
}

/// 插入可参与分流的账号；令牌有效期留空，表示沿用现有未知到期时间的兼容语义。
fn insertAccount(storage: &Storage, id: &str, sort: i64) {
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: id.to_string(),
            label: format!("账号 {id}"),
            issuer: "chatgpt".to_string(),
            chatgpt_account_id: Some(format!("workspace-{id}")),
            workspace_id: Some(format!("workspace-{id}")),
            group_name: None,
            sort,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("插入账号");
    storage
        .insert_token(&Token {
            account_id: id.to_string(),
            id_token: String::new(),
            access_token: format!("access-{id}"),
            refresh_token: String::new(),
            api_key_access_token: None,
            last_refresh: now,
        })
        .expect("插入令牌");
}

#[test]
fn newSessionsUseLeastLoadedAccountsAndRemainSticky() {
    let mut storage = storage();
    insertAccount(&storage, "account-a", 0);
    insertAccount(&storage, "account-b", 1);

    let first = storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("分配首个会话");
    let second = storage
        .resolveSessionRouting("thread-b", "thread-id", now_ts())
        .expect("分配第二个会话");
    let repeated = storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("读取固定绑定");

    let SessionRoutingResolution::Routed(first) = first else {
        panic!("首个会话未路由");
    };
    let SessionRoutingResolution::Routed(second) = second else {
        panic!("第二个会话未路由");
    };
    let SessionRoutingResolution::Routed(repeated) = repeated else {
        panic!("重复会话未路由");
    };
    assert_ne!(first.account_id, second.account_id);
    assert_eq!(first.account_id, repeated.account_id);
}

#[test]
fn disabledAccountIsExcludedFromNewSessions() {
    let mut storage = storage();
    insertAccount(&storage, "account-a", 0);
    insertAccount(&storage, "account-b", 1);
    assert!(storage
        .setAccountSessionRoutingEnabled("account-a", false, now_ts())
        .expect("停用账号分流"));

    let resolution = storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("分配会话");
    let SessionRoutingResolution::Routed(credential) = resolution else {
        panic!("会话未路由");
    };
    assert_eq!(credential.account_id, "account-b");
}

#[test]
fn disabledBoundAccountDoesNotSwitchSilently() {
    let mut storage = storage();
    insertAccount(&storage, "account-a", 0);
    let first = storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("创建绑定");
    let SessionRoutingResolution::Routed(first) = first else {
        panic!("会话未路由");
    };
    assert!(storage
        .setAccountSessionRoutingEnabled(&first.account_id, false, now_ts())
        .expect("停用绑定账号"));

    let repeated = storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("读取失效绑定");
    assert_eq!(
        repeated,
        SessionRoutingResolution::BoundAccountUnavailable {
            account_id: first.account_id,
            reason: "account_routing_disabled".to_string(),
        }
    );
}

#[test]
fn preferenceSummaryDefaultsToEnabledAndCountsBindings() {
    let mut storage = storage();
    insertAccount(&storage, "account-a", 0);
    storage
        .resolveSessionRouting("thread-a", "thread-id", now_ts())
        .expect("创建绑定");

    assert_eq!(
        storage
            .listAccountRoutingPreferences()
            .expect("读取分流摘要"),
        vec![AccountRoutingPreference {
            account_id: "account-a".to_string(),
            enabled: true,
            active_binding_count: 1,
        }]
    );
}
