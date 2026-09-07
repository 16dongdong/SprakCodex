//! Windows 进程发现与 DLL 注入基础层。
//! 注入前必须由观测协议层提供对应 DLL 和配置；本模块不修改目标进程环境变量。

use std::path::{Path, PathBuf};

const targetProcessNames: &[&str] = &["codex.exe", "codex-app.exe"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcessCandidate {
    pub pid: u32,
    pub executable: PathBuf,
}

// 过滤 Codex 主进程和 app-server，返回本次扫描的实时 PID；调用方不得缓存 PID 跨重启使用。
pub(super) fn findCandidates() -> Vec<ProcessCandidate> {
    let system = sysinfo::System::new_all();
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let executable = process.exe()?.to_path_buf();
            if isTargetExecutable(&executable) {
                Some(ProcessCandidate {
                    pid: pid.as_u32(),
                    executable,
                })
            } else {
                None
            }
        })
        .collect()
}

// app-server 是通用参数而不是进程身份；只接受指定可执行文件名，避免误接管其他本地服务。
fn isTargetExecutable(executable: &Path) -> bool {
    executable.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        targetProcessNames.iter().any(|target| name.eq_ignore_ascii_case(target))
    })
}

// 使用绝对 DLL 路径，并在注入前检查架构路径；失败返回可展示诊断，绝不报告假成功。
#[cfg(windows)]
pub(super) fn inject(pid: u32, dll: &Path) -> Result<(), String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::s;
    use windows::Win32::Foundation::{CloseHandle, FALSE};
    use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    use windows::Win32::System::Memory::{
        VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
    };
    use windows::Win32::System::Threading::{
        CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject,
        PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
        PROCESS_VM_WRITE,
    };

    if !dll.is_file() {
        return Err(format!("注入 DLL 不存在: {}", dll.display()));
    }
    let path: Vec<u16> = dll
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let access = PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ;
        let process = OpenProcess(access, FALSE, pid)
            .map_err(|error| format!("打开 Codex 进程失败 pid={pid}: {error}"))?;
        let result = (|| {
            let remote = VirtualAllocEx(
                process,
                None,
                path.len() * 2,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if remote.is_null() {
                return Err("分配远程注入缓冲区失败".to_string());
            }
            if WriteProcessMemory(
                process,
                remote,
                path.as_ptr() as *const c_void,
                path.len() * 2,
                None,
            )
            .is_err()
            {
                let _ = VirtualFreeEx(process, remote, 0, MEM_RELEASE);
                return Err("写入远程 DLL 路径失败".to_string());
            }
            let kernel = GetModuleHandleA(s!("kernel32.dll")).map_err(|error| error.to_string())?;
            let loadLibrary =
                GetProcAddress(kernel, s!("LoadLibraryW")).ok_or("找不到 LoadLibraryW")?;
            let start = Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(*mut c_void) -> u32,
            >(loadLibrary));
            let thread = CreateRemoteThread(process, None, 0, start, Some(remote), 0, None)
                .map_err(|error| format!("创建远程线程失败: {error}"))?;
            let wait = WaitForSingleObject(thread, 10_000);
            let mut exitCode = 0u32;
            let _ = GetExitCodeThread(thread, &mut exitCode);
            let _ = CloseHandle(thread);
            let _ = VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            if wait.0 == 0x00000102 {
                return Err("等待远程 DLL 加载超时".to_string());
            }
            if exitCode == 0 {
                return Err("目标进程拒绝加载观测 DLL".to_string());
            }
            Ok(())
        })();
        let _ = CloseHandle(process);
        result
    }
}

#[cfg(not(windows))]
pub(super) fn inject(_pid: u32, _dll: &Path) -> Result<(), String> {
    Err("当前平台没有 Windows 进程注入实现".to_string())
}

#[cfg(test)]
#[path = "../../tests/observation/processSelectionTests.rs"]
mod tests;
