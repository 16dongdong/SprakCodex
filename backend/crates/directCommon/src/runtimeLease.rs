//! 用内核线程对象判断 Relay 运行实例寿命；只查询和等待，不修改线程或目标进程。
use crate::relayContract::RuntimeIdentity;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::{
    Foundation::{FILETIME, HANDLE, WAIT_TIMEOUT},
    System::Threading::{
        GetCurrentProcessId, GetCurrentThread, GetCurrentThreadId, GetProcessIdOfThread,
        GetThreadTimes, OpenThread, WaitForSingleObject, THREAD_QUERY_LIMITED_INFORMATION,
        THREAD_SYNCHRONIZE,
    },
};

// 句柄固定内核线程实例；宿主异常退出后即使 ID 已复用，旧句柄也保持已终止状态。
pub struct RuntimeLease(OwnedHandle);

// 在专用 Relay 线程发布配置时调用；时间读取失败时阻止启动，不发布没有所有者的有效端口。
pub fn currentIdentity() -> Result<RuntimeIdentity, &'static str> {
    unsafe {
        Ok(RuntimeIdentity {
            processId: GetCurrentProcessId(),
            threadId: GetCurrentThreadId(),
            createdAt: creationTime(GetCurrentThread())?,
        })
    }
}

impl RuntimeLease {
    // 只接受进程 ID、线程 ID 和原始创建时间均匹配的实例；退出、拒绝访问或身份错误返回失败。
    pub fn open(identity: RuntimeIdentity) -> Result<Self, &'static str> {
        let thread = unsafe {
            OpenThread(
                THREAD_QUERY_LIMITED_INFORMATION | THREAD_SYNCHRONIZE,
                false,
                identity.threadId,
            )
        }
        .map_err(|_| "打开 Relay 运行线程失败")?;
        let lease = Self(unsafe { OwnedHandle::from_raw_handle(thread.0) });
        if unsafe { GetProcessIdOfThread(thread) } != identity.processId
            || creationTime(thread)? != identity.createdAt
            || !lease.isActive()
        {
            return Err("Relay 运行线程身份不匹配或已退出");
        }
        Ok(lease)
    }

    // 每次决定新连接是否改连前进行零超时等待；只有明确仍在运行才允许使用缓存端口。
    pub fn isActive(&self) -> bool {
        unsafe { WaitForSingleObject(HANDLE(self.0.as_raw_handle()), 0) == WAIT_TIMEOUT }
    }
}

// 完整 FILETIME 参与实例比较，禁止转换为秒或使用进程枚举缓存造成 ID 复用误判。
fn creationTime(thread: HANDLE) -> Result<u64, &'static str> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetThreadTimes(thread, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|_| "读取 Relay 运行线程创建时间失败")?;
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

#[cfg(test)]
#[path = "../tests/unit/runtimeLeaseTests.rs"]
mod tests;
