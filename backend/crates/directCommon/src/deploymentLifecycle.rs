//! 宿主与独立看门狗共享的内存映像卸载记录；进程创建时间阻止 PID 复用误操作。
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentRecord {
    pub processId: u32,
    pub createdAt: u64,
    pub imageBase: usize,
    pub imageSize: usize,
    pub entryPoint: usize,
    pub shutdownEntry: usize,
    pub functionTable: usize,
}

impl DeploymentRecord {
    // 所有远程地址必须落在同一已知映像；异常表允许为空，其余零值或溢出记录一律拒绝。
    pub fn validate(&self) -> Result<(), &'static str> {
        let end = self
            .imageBase
            .checked_add(self.imageSize)
            .filter(|end| self.imageBase != 0 && self.imageSize != 0 && *end > self.imageBase)
            .ok_or("观测部署映像范围无效")?;
        if self.processId == 0
            || self.createdAt == 0
            || !(self.imageBase..end).contains(&self.entryPoint)
            || !(self.imageBase..end).contains(&self.shutdownEntry)
            || (self.functionTable != 0 && !(self.imageBase..end).contains(&self.functionTable))
        {
            return Err("观测部署记录无效");
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnloadContext {
    pub imageBase: usize,
    pub entryPoint: usize,
    pub functionTable: usize,
}

#[cfg(windows)]
mod windowsUnload {
    use super::{DeploymentRecord, UnloadContext};
    use std::{
        ffi::c_void,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows::Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, FILETIME, HANDLE, WAIT_OBJECT_0},
        System::{
            Diagnostics::Debug::WriteProcessMemory,
            Memory::{
                VirtualAllocEx, VirtualFreeEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION,
                MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
            },
            Threading::{
                CreateRemoteThread, GetExitCodeThread, GetProcessTimes, OpenProcess,
                WaitForSingleObject, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION,
                PROCESS_SYNCHRONIZE, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
            },
        },
    };

    #[allow(non_upper_case_globals)]
    const shutdownTimeoutMs: u32 = 15_000;

    // 成功句柄立即转交 RAII，任何验证或远程调用失败都不会泄漏本地句柄。
    unsafe fn own(value: HANDLE) -> OwnedHandle {
        OwnedHandle::from_raw_handle(value.0)
    }

    // OwnedHandle 只借给 Win32 API，本函数不转移关闭责任。
    fn native(value: &OwnedHandle) -> HANDLE {
        HANDLE(value.as_raw_handle())
    }

    // 目标退出后系统会回收全部私有映像；后续 API 失败应折叠为幂等成功，而不是阻断主程序更新。
    fn processExited(process: HANDLE) -> bool {
        (unsafe { WaitForSingleObject(process, 0) }) == WAIT_OBJECT_0
    }

    // 创建时间来自同一进程句柄，目标退出或 PID 已复用时拒绝执行远程入口。
    fn creationTime(process: HANDLE) -> Result<u64, String> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            .map_err(|_| "读取观测目标创建时间失败")?;
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    // 映像必须仍以同一 allocation base 提交；已释放区域返回 false，使重复清理保持幂等。
    fn isMapped(process: HANDLE, record: &DeploymentRecord) -> Result<bool, String> {
        let mut information = MEMORY_BASIC_INFORMATION::default();
        let size = unsafe {
            VirtualQueryEx(
                process,
                Some(record.imageBase as *const c_void),
                &mut information,
                std::mem::size_of_val(&information),
            )
        };
        if size != std::mem::size_of_val(&information) {
            if processExited(process) {
                return Ok(false);
            }
            return Err("读取观测映像状态失败".into());
        }
        Ok(information.State == MEM_COMMIT
            && information.AllocationBase as usize == record.imageBase)
    }

    // 看门狗只调用记录中的固定 shutdown ABI；上下文保留到远程线程退出，不产生悬空参数。
    pub(super) fn unload(record: &DeploymentRecord) -> Result<bool, String> {
        record.validate().map_err(str::to_owned)?;
        let access = PROCESS_QUERY_INFORMATION
            | PROCESS_CREATE_THREAD
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ
            | PROCESS_SYNCHRONIZE;
        let process = match unsafe { OpenProcess(access, false, record.processId) } {
            Ok(process) => unsafe { own(process) },
            Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Ok(false),
            Err(_) => return Err("打开观测卸载目标失败".into()),
        };
        if creationTime(native(&process))? != record.createdAt {
            return Err("观测目标进程实例已变化".into());
        }
        if processExited(native(&process)) {
            return Ok(false);
        }
        if !isMapped(native(&process), record)? {
            return Ok(false);
        }
        let context = UnloadContext {
            imageBase: record.imageBase,
            entryPoint: record.entryPoint,
            functionTable: record.functionTable,
        };
        let contextAddress = unsafe {
            VirtualAllocEx(
                native(&process),
                None,
                std::mem::size_of::<UnloadContext>(),
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if contextAddress.is_null() {
            if processExited(native(&process)) {
                return Ok(false);
            }
            return Err("分配观测卸载上下文失败".into());
        }
        let mut written = 0;
        let writeResult = unsafe {
            WriteProcessMemory(
                native(&process),
                contextAddress,
                (&context as *const UnloadContext).cast(),
                std::mem::size_of::<UnloadContext>(),
                Some(&mut written),
            )
        };
        if writeResult.is_err() || written != std::mem::size_of::<UnloadContext>() {
            if processExited(native(&process)) {
                return Ok(false);
            }
            if unsafe { VirtualFreeEx(native(&process), contextAddress, 0, MEM_RELEASE) }.is_err() {
                return Err("写入观测卸载上下文失败且临时内存回收失败".into());
            }
            return Err("写入观测卸载上下文失败".into());
        }
        let entry = unsafe {
            std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(
                record.shutdownEntry,
            )
        };
        let thread = match unsafe {
            CreateRemoteThread(
                native(&process),
                None,
                0,
                Some(entry),
                Some(contextAddress.cast_const()),
                0,
                None,
            )
        } {
            Ok(thread) => unsafe { own(thread) },
            Err(_) => {
                if processExited(native(&process)) {
                    return Ok(false);
                }
                if unsafe { VirtualFreeEx(native(&process), contextAddress, 0, MEM_RELEASE) }
                    .is_err()
                {
                    return Err("创建观测卸载线程失败且临时内存回收失败".into());
                }
                return Err("创建观测卸载线程失败".into());
            }
        };
        if unsafe { WaitForSingleObject(native(&thread), shutdownTimeoutMs) } != WAIT_OBJECT_0 {
            if processExited(native(&process)) {
                return Ok(false);
            }
            return Err("等待观测模块排空超时".into());
        }
        let mut result = 0;
        let exitResult = unsafe { GetExitCodeThread(native(&thread), &mut result) }
            .map_err(|_| "读取观测卸载结果失败".to_string());
        let freeResult = unsafe { VirtualFreeEx(native(&process), contextAddress, 0, MEM_RELEASE) }
            .map_err(|_| "释放观测卸载上下文失败".to_string());
        if (exitResult.is_err() || freeResult.is_err()) && processExited(native(&process)) {
            return Ok(false);
        }
        exitResult?;
        freeResult?;
        if result != 1 {
            return Err("观测模块拒绝卸载".into());
        }
        let released = unsafe {
            VirtualFreeEx(
                native(&process),
                record.imageBase as *mut c_void,
                0,
                MEM_RELEASE,
            )
        };
        if released.is_err() {
            if processExited(native(&process)) {
                return Ok(false);
            }
            return Err("释放观测内存映像失败".into());
        }
        if isMapped(native(&process), record)? {
            return Err("观测内存映像释放后仍可见".into());
        }
        Ok(true)
    }
}

// 返回 true 表示本次释放了映像，false 表示目标已退出或映像已不存在。
#[cfg(windows)]
pub fn unload(record: &DeploymentRecord) -> Result<bool, String> {
    windowsUnload::unload(record)
}

// 非 Windows 平台没有原生观测映像；调用保持幂等并报告没有执行释放。
#[cfg(not(windows))]
pub fn unload(_: &DeploymentRecord) -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::DeploymentRecord;

    // 生命周期地址必须全部属于同一映像，进程身份、大小和关键入口均不接受零值。
    #[test]
    fn deploymentRecordRejectsForeignAddresses() {
        let valid = DeploymentRecord {
            processId: 42,
            createdAt: 77,
            imageBase: 0x1000,
            imageSize: 0x1000,
            entryPoint: 0x1100,
            shutdownEntry: 0x1200,
            functionTable: 0x1300,
        };
        assert!(valid.validate().is_ok());
        for invalid in [
            DeploymentRecord {
                shutdownEntry: 0x3000,
                ..valid.clone()
            },
            DeploymentRecord {
                entryPoint: 0,
                ..valid.clone()
            },
            DeploymentRecord {
                imageSize: 0,
                ..valid.clone()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}
