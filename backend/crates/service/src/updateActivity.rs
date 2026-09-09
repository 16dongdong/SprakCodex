use std::sync::Mutex;

// 请求计数与排空状态在同一临界区变更，避免空闲检查和新请求之间存在窗口。
#[derive(Default)]
struct ActivityState {
    active: usize,
    draining: bool,
}
impl ActivityState {
    // 排空期间拒绝新增工作，计数溢出也视为不可接收。
    fn begin(&mut self) -> Result<(), ()> {
        if self.draining {
            return Err(());
        }
        self.active = self.active.checked_add(1).ok_or(())?;
        Ok(())
    }
    // 只有无活动任务时才切换排空；重复更新请求不共享退出许可。
    fn drain(&mut self) -> bool {
        if self.active != 0 || self.draining {
            return false;
        }
        self.draining = true;
        true
    }
}
static ACTIVITY: Mutex<ActivityState> = Mutex::new(ActivityState {
    active: 0,
    draining: false,
});
pub(crate) struct RequestLease;

// 已开始的请求由 RAII 保持活跃，直至响应与日志提交完成；排空期间拒绝新请求。
pub(crate) fn begin_request() -> Result<RequestLease, ()> {
    ACTIVITY.lock().map_err(|_| ())?.begin()?;
    Ok(RequestLease)
}
impl Drop for RequestLease {
    // 租约只释放自己的计数；锁损坏时保留错误状态，使更新检查关闭失败。
    fn drop(&mut self) {
        if let Ok(mut state) = ACTIVITY.lock() {
            state.active = state.active.saturating_sub(1);
        }
    }
}
// 空闲时原子禁止新请求；仍有生成任务时返回 false，调用方稍后重试而非强制终止。
pub fn begin_update_drain() -> Result<bool, String> {
    Ok(ACTIVITY.lock().map_err(|_| "请求生命周期锁损坏")?.drain())
}
// 更新器尚未就绪时解除排空，让现有服务继续接收请求。
pub fn cancel_update_drain() {
    if let Ok(mut state) = ACTIVITY.lock() {
        state.draining = false;
    }
}
#[cfg(test)]
#[path = "../tests/updateActivity/stateTests.rs"]
mod tests;
