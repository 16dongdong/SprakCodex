use codexmanager_core::storage::now_ts;
use std::{sync::OnceLock, thread, time::Duration};

static STARTED: OnceLock<()> = OnceLock::new();
const POLL_SECONDS: u64 = 5;

// 服务进程拥有调度器，页面关闭不影响执行；单工作线程限制并发，SQLite 领取状态负责跨重启去重。
pub(crate) fn start() {
    STARTED.get_or_init(|| {
        thread::spawn(|| loop {
            if let Err(error) = cycle() { log::warn!("重置后预热失败: {error}"); }
            thread::sleep(Duration::from_secs(POLL_SECONDS));
        });
    });
}

// 从持久化快照恢复周期，先消费旧任务再观察新周期，避免到期时刷新把旧截止时间覆盖。
// 一轮最多处理一个账号；预热前后分别刷新额度，以恢复候选资格并读取真实的新截止时间。
fn cycle() -> Result<(), String> {
    // 更新排空时不领取任务，避免已标记领取却在重启前被打断。
    let Ok(_lease) = crate::updateActivity::begin_request() else { return Ok(()); };
    let mut storage = crate::storage_helpers::open_storage().ok_or("额度调度存储不可用")?;
    if let Some(accountId) = storage.claimResetWarmup(now_ts()).map_err(|e| e.to_string())? {
        crate::usage_refresh::refresh_usage_for_account(&accountId)?;
        let result = crate::account_warmup::warmup_accounts(vec![accountId.clone()], "")?;
        crate::usage_refresh::refresh_usage_for_account(&accountId)?;
        if result.failed > 0 { return Err("账号预热未成功，请查看预热请求日志后手动重试".into()); }
    }
    for snapshot in storage.latest_usage_snapshots_by_account().map_err(|e| e.to_string())? {
        storage.observeResetWarmup(&snapshot).map_err(|e| e.to_string())?;
    }
    Ok(())
}
