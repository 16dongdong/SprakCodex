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
fn disabledBoundAccountWaitsWhenNoReplacementExists() {
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
    assert_eq!(repeated, SessionRoutingResolution::NoAvailableAccount);
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

// 手动换号先标记待切换，旧连接失效；新连接使用指定账号，重复请求保持绑定。
#[test]
fn manualSwitchAndResetInvalidateOldConnection() {
    let mut storage = storage();
    insertAccount(&storage, "a", 0);
    insertAccount(&storage, "b", 1);
    storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap();
    assert!(storage
        .routingConnectionCurrent("session", "a", now_ts())
        .unwrap());
    assert!(storage
        .switchRoutingSession("session", "b", now_ts())
        .unwrap());
    assert!(!storage
        .routingConnectionCurrent("session", "a", now_ts())
        .unwrap());
    let SessionRoutingResolution::Routed(route) = storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap()
    else {
        panic!("未完成迁移")
    };
    assert_eq!(route.account_id, "b");
    assert!(storage.resetRoutingSession("session").unwrap());
    assert!(!storage
        .routingConnectionCurrent("session", "b", now_ts())
        .unwrap());
    assert_eq!(storage.listRoutingSessions(1, "", "").unwrap()["total"], 0);
}

// 删除账号保留会话元数据，缺少候选时等待，新增候选后自动迁移。
#[test]
fn deletedAccountKeepsPendingSessionAndMigrates() {
    let mut storage = storage();
    insertAccount(&storage, "a", 0);
    storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap();
    storage
        .conn
        .execute("DELETE FROM accounts WHERE id='a'", [])
        .unwrap();
    assert_eq!(
        storage.listRoutingSessions(1, "", "").unwrap()["items"][0]["reason"],
        "account_deleted"
    );
    assert_eq!(
        storage
            .resolveSessionRouting("session", "thread-id", now_ts())
            .unwrap(),
        SessionRoutingResolution::NoAvailableAccount
    );
    insertAccount(&storage, "b", 1);
    let SessionRoutingResolution::Routed(route) = storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap()
    else {
        panic!("未分配候选")
    };
    assert_eq!(route.account_id, "b");
}

// 明确额度失败后排除原账号；旧响应不自动重放，新请求选择另一可用账号。
#[test]
fn exhaustedAccountMigratesOnNextRequest() {
    let mut storage = storage();
    insertAccount(&storage, "a", 0);
    insertAccount(&storage, "b", 1);
    storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap();
    storage
        .recordSessionQuotaFailure("workspace-a", now_ts() + 300, now_ts())
        .unwrap();
    assert!(!storage
        .routingConnectionCurrent("session", "a", now_ts())
        .unwrap());
    let SessionRoutingResolution::Routed(route) = storage
        .resolveSessionRouting("session", "thread-id", now_ts())
        .unwrap()
    else {
        panic!("额度迁移失败")
    };
    assert_eq!(route.account_id, "b");
}

// 7 天窗口位于主槽位时仍计入周额度，缺失的 5 小时窗口不作为零或满额度参加平均。
#[test]
fn quotaOverviewUsesWindowDurationRatherThanSlot() {
    let storage = storage();
    insertAccount(&storage, "a", 0);
    insertAccount(&storage, "b", 1);
    storage.conn.execute("INSERT INTO usage_snapshots(account_id,used_percent,window_minutes,captured_at) VALUES('a',40,10080,1)",[]).unwrap();
    storage.conn.execute("INSERT INTO usage_snapshots(account_id,used_percent,window_minutes,secondary_used_percent,secondary_window_minutes,captured_at) VALUES('b',3,300,0,10080,1)",[]).unwrap();
    let summary = storage.account_quota_overview_stats().unwrap();
    assert_eq!(summary.primary_remain_percent_avg, Some(97.0));
    assert_eq!(summary.secondary_remain_percent_avg, Some(80.0));
}

// 显示缓存写入不改变绑定；标题与项目过滤在分页前执行，不把当前页外的匹配会话漏掉。
#[test]
fn metadataSearchAndProjectFilterPreserveBinding() {
    let mut storage=storage();insertAccount(&storage,"a",0);
    storage.resolveSessionRouting("session","thread-id",now_ts()).unwrap();
    storage.syncRoutingDisplay(&[("session".into(),"检查网络配置".into(),"D:/workspace/project".into())]).unwrap();
    let page=storage.listRoutingSessions(1,"网络","D:/workspace/project").unwrap();
    assert_eq!(page["total"],1);
    assert_eq!(page["items"][0]["title"],"检查网络配置");
    assert_eq!(page["items"][0]["accountId"],"a");
    assert_eq!(storage.listRoutingSessions(1,"网络","D:/other").unwrap()["total"],0);
}
