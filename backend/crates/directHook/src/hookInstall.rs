//! Detour 所有权、原调用地址和卸载门控集中管理；入口恢复后才释放 trampoline。
use retour::RawDetour;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

static unloading: AtomicBool = AtomicBool::new(false);
static activeCallbacks: AtomicUsize = AtomicUsize::new(0);

pub(super) struct DetourSlot {
    owner: Mutex<Option<RawDetour>>,
    trampoline: AtomicUsize,
}

impl DetourSlot {
    // 静态槽以空所有权和空原调用地址启动，可安全放入进程级全局状态。
    pub(super) const fn new() -> Self {
        Self {
            owner: Mutex::new(None),
            trampoline: AtomicUsize::new(0),
        }
    }

    // 安装先发布 owner 与 trampoline，再改写目标入口；失败会回收已创建的可执行页。
    pub(super) unsafe fn install(
        &self,
        target: *const (),
        callback: *const (),
    ) -> Result<(), String> {
        let mut owner = self.owner.lock().map_err(|_| "Detour 状态锁损坏")?;
        if owner.is_some() {
            return Err("原调用槽已初始化，拒绝重复安装".into());
        }
        let detour = RawDetour::new(target, callback).map_err(|_| "创建原生入口失败")?;
        self.trampoline
            .store(detour.trampoline() as *const () as usize, Ordering::Release);
        *owner = Some(detour);
        if let Err(error) = owner.as_ref().expect("Detour 所有权已发布").enable() {
            self.trampoline.store(0, Ordering::Release);
            owner.take();
            return Err(format!("启用原生入口失败：{error}"));
        }
        Ok(())
    }

    // 回调只读取安装完成后不变的 trampoline；零值表示初始化或卸载协议被破坏。
    pub(super) fn original<F: Copy>(&self) -> F {
        let address = self.trampoline.load(Ordering::Acquire);
        assert_ne!(address, 0, "原调用槽未初始化");
        unsafe { std::mem::transmute_copy(&address) }
    }

    // 第一步只恢复目标入口，保留 trampoline 供已经进入回调的线程完成原调用。
    pub(super) fn disable(&self) -> Result<(), String> {
        let owner = self.owner.lock().map_err(|_| "Detour 状态锁损坏")?;
        if let Some(detour) = owner.as_ref() {
            unsafe { detour.disable() }.map_err(|_| "恢复原生入口失败")?;
        }
        Ok(())
    }

    // 所有回调排空后释放 trampoline／relay 可执行页；此后槽不再允许原调用。
    pub(super) fn release(&self) -> Result<(), String> {
        let mut owner = self.owner.lock().map_err(|_| "Detour 状态锁损坏")?;
        if owner.take().is_some() {
            self.trampoline.store(0, Ordering::Release);
        }
        Ok(())
    }

    // 单元测试只观察所有权状态，不暴露可执行页地址或允许业务代码绕过生命周期。
    #[cfg(test)]
    pub(super) fn isInstalled(&self) -> bool {
        self.owner.lock().is_ok_and(|owner| owner.is_some())
    }

    // 单元测试只建立 trampoline，不改写进程级系统入口；测试结束由 release 回收。
    #[cfg(test)]
    pub(super) unsafe fn prepareForTest(
        &self,
        target: *const (),
        callback: *const (),
    ) -> Result<(), String> {
        let mut owner = self.owner.lock().map_err(|_| "Detour 状态锁损坏")?;
        if owner.is_some() {
            return Err("原调用槽已初始化，拒绝重复安装".into());
        }
        let detour = RawDetour::new(target, callback).map_err(|_| "创建原生入口失败")?;
        self.trampoline
            .store(detour.trampoline() as *const () as usize, Ordering::Release);
        *owner = Some(detour);
        Ok(())
    }
}

pub(super) struct CallbackActivity {
    unloading: bool,
}

impl CallbackActivity {
    // 计数先于卸载状态读取，保证已进入模块的回调在 trampoline 释放前可见。
    pub(super) fn enter() -> Self {
        activeCallbacks.fetch_add(1, Ordering::AcqRel);
        Self {
            unloading: unloading.load(Ordering::Acquire),
        }
    }

    // 卸载中的回调只执行原函数，不再读取配置或修改模块状态。
    pub(super) fn isUnloading(&self) -> bool {
        self.unloading
    }
}

impl Drop for CallbackActivity {
    // Release 与卸载线程的 Acquire 读取配对，最后一个回调退出后即可释放代码页。
    fn drop(&mut self) {
        activeCallbacks.fetch_sub(1, Ordering::AcqRel);
    }
}

// 设置后不可恢复；同一映像只允许卸载一次，新宿主会部署新的映像实例。
pub(super) fn beginUnload() -> bool {
    unloading
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

// 调用方已恢复全部入口；连续空闲窗口用于跨过正在从目标序言跳入回调的极短边界。
pub(super) fn waitForCallbacks() -> bool {
    const stableChecks: usize = 20;
    const checkDelay: std::time::Duration = std::time::Duration::from_millis(10);
    let mut stable = 0;
    for _ in 0..1_000 {
        if activeCallbacks.load(Ordering::Acquire) == 0 {
            stable += 1;
            if stable == stableChecks {
                return true;
            }
        } else {
            stable = 0;
        }
        std::thread::sleep(checkDelay);
    }
    false
}

#[cfg(test)]
#[path = "../tests/unit/detourLifecycleTests.rs"]
mod tests;
