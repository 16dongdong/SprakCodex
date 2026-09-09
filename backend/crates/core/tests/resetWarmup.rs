#![allow(non_snake_case)]
use codexmanager_core::storage::{Account, Storage, UsageSnapshotRecord};

// 测试仅使用内存数据库和虚构时间，不发送预热或兑换请求。
fn fixture(status: &str) -> Storage {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    storage.insert_account(&Account {
        id: "fixture".into(), label: "fixture".into(), issuer: "fixture".into(),
        chatgpt_account_id: None, workspace_id: None, group_name: None,
        sort: 0, status: status.into(), created_at: 1, updated_at: 1,
    }).unwrap();
    storage
}

// 两个窗口刻意放在相反槽位，确认调度依赖持续时间而不是主次槽位名称。
fn snapshot(deadline: i64, captured: i64) -> UsageSnapshotRecord {
    UsageSnapshotRecord {
        account_id: "fixture".into(), used_percent: Some(10.0), window_minutes: Some(10080), resets_at: Some(deadline),
        secondary_used_percent: Some(20.0), secondary_window_minutes: Some(300), secondary_resets_at: Some(deadline),
        credits_json: None, captured_at: captured,
    }
}

// 截止边界只领取一次，同时到期的两个额度合并，重复快照不会重新激活任务。
#[test]
fn deadlineAndDeduplication() {
    let mut storage = fixture("active");
    storage.observeResetWarmup(&snapshot(100, 1)).unwrap();
    assert!(storage.claimResetWarmup(99).unwrap().is_none());
    assert_eq!(storage.claimResetWarmup(100).unwrap().as_deref(), Some("fixture"));
    storage.observeResetWarmup(&snapshot(100, 101)).unwrap();
    assert!(storage.claimResetWarmup(101).unwrap().is_none());
    storage.observeResetWarmup(&snapshot(200, 101)).unwrap();
    assert!(storage.claimResetWarmup(199).unwrap().is_none());
    assert!(storage.claimResetWarmup(200).unwrap().is_some());
}

// 新快照不能覆盖已经到期的待执行任务；领取完成后再注册下一周期。
#[test]
fn rolloverPreservesPendingDeadline() {
    let mut storage = fixture("active");
    storage.observeResetWarmup(&snapshot(100, 1)).unwrap();
    storage.observeResetWarmup(&snapshot(200, 110)).unwrap();
    assert!(storage.claimResetWarmup(110).unwrap().is_some());
    storage.observeResetWarmup(&snapshot(200, 111)).unwrap();
    assert!(storage.claimResetWarmup(199).unwrap().is_none());
    assert!(storage.claimResetWarmup(200).unwrap().is_some());
}

// 未提供截止时间不猜测周期；停用账号不触发自动调用。
#[test]
fn missingDeadlineAndDisabledAccount() {
    let mut storage = fixture("disabled");
    let mut missing = snapshot(100, 1);
    missing.resets_at = None;
    missing.secondary_resets_at = None;
    storage.observeResetWarmup(&missing).unwrap();
    assert!(storage.claimResetWarmup(100).unwrap().is_none());
    storage.enqueueResetWarmup("fixture", 100).unwrap();
    assert!(storage.claimResetWarmup(100).unwrap().is_none());
}

// 手动重置与自然到期合并，重复确认不再次调用，新一次重置则产生新任务。
#[test]
fn manualResetCoalesces() {
    let mut storage = fixture("active");
    storage.observeResetWarmup(&snapshot(100, 1)).unwrap();
    storage.enqueueResetWarmup("fixture", 100).unwrap();
    assert!(storage.claimResetWarmup(100).unwrap().is_some());
    storage.enqueueResetWarmup("fixture", 100).unwrap();
    assert!(storage.claimResetWarmup(101).unwrap().is_none());
    storage.enqueueResetWarmup("fixture", 102).unwrap();
    assert!(storage.claimResetWarmup(102).unwrap().is_some());
}
