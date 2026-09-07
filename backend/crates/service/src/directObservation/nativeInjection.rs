//! Windows 加载事务：验证进程实例与架构，等待模块就绪；超时仍由清理线程持有远程参数，避免释放正在使用的内存。
use super::processInjector::ProcessCandidate;
use std::{
    collections::HashSet,
    ffi::c_void,
    os::windows::ffi::OsStrExt,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use windows::{
    core::{s, PWSTR},
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, FILETIME, HANDLE, HMODULE, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        System::{
            Diagnostics::{
                Debug::WriteProcessMemory,
                ToolHelp::{
                    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W,
                    TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
                },
            },
            LibraryLoader::{
                GetModuleHandleExW, GetModuleHandleW, GetProcAddress,
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
            Memory::{
                VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
            },
            SystemInformation::IMAGE_FILE_MACHINE,
            Threading::{
                CreateRemoteThread, GetCurrentProcess, GetProcessTimes, IsWow64Process2,
                OpenEventW, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
                PROCESS_CREATE_THREAD, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_OPERATION,
                PROCESS_VM_READ, PROCESS_VM_WRITE, SYNCHRONIZATION_SYNCHRONIZE,
            },
        },
    },
};

const loadTimeoutMs: u32 = 10_000;
const readyTimeout: Duration = Duration::from_secs(8);
const cleanupInterval: Duration = Duration::from_millis(100);
const maxPendingLoads: usize = 16;
static pendingLoads: Mutex<Option<HashSet<(u32, u64)>>> = Mutex::new(None);

// 加载预留与远程对象同寿命；Manager 重新扫描时不重复创建仍在运行的 LoadLibrary 线程。
struct Reservation {
    identity: (u32, u64),
}
impl Reservation {
    // 同时限制活跃和延迟清理任务总数；重复实例或超限返回错误，不分配远程内存。
    fn acquire(candidate: &ProcessCandidate) -> Result<Self, String> {
        let mut guard = pendingLoads.lock().map_err(|_| "注入任务状态锁损坏")?;
        let active = guard.get_or_insert_with(HashSet::new);
        let identity = (candidate.pid, candidate.createdAt);
        if active.len() >= maxPendingLoads || !active.insert(identity) {
            return Err("目标加载尚未完成或等待队列已满".into());
        }
        Ok(Self { identity })
    }
}
impl Drop for Reservation {
    // 正常完成与延迟清理共用释放路径；析构中不触发 panic。
    fn drop(&mut self) {
        if let Ok(mut guard) = pendingLoads.lock() {
            if let Some(active) = guard.as_mut() {
                active.remove(&self.identity);
            }
        }
    }
}

// 进程句柄固定实例而不是 PID；即使 PID 被复用，释放操作也只作用于原进程对象。
struct RemoteAllocation {
    process: OwnedHandle,
    address: usize,
}
impl Drop for RemoteAllocation {
    // 仅在加载线程终止后析构；进程已退出时其地址空间由 Windows 回收。
    fn drop(&mut self) {
        let process = native(&self.process);
        unsafe {
            if WaitForSingleObject(process, 0) != WAIT_OBJECT_0
                && VirtualFreeEx(process, self.address as *mut c_void, 0, MEM_RELEASE).is_err()
            {
                log::error!("释放观测加载参数失败");
            }
        }
    }
}

// 加载线程、参数内存、任务预留绑定在同一对象内，禁止提前释放其中任一依赖。
struct RemoteLoad {
    thread: OwnedHandle,
    allocation: RemoteAllocation,
    reservation: Reservation,
}

// 将已成功创建的 Windows 句柄移交标准库 RAII；调用方只传入非空、非 INVALID_HANDLE_VALUE 的有效句柄。
unsafe fn own(value: HANDLE) -> OwnedHandle {
    OwnedHandle::from_raw_handle(value.0)
}

// 仅借用句柄供 Windows API 调用，不转移其所有权。
fn native(value: &OwnedHandle) -> HANDLE {
    HANDLE(value.as_raw_handle())
}

// 最小权限读取实例身份；路径和创建时间均从同一进程句柄读取，不把文件名相同当成同一实例。
pub(super) fn candidate(pid: u32) -> Result<ProcessCandidate, String> {
    let process = unsafe {
        own(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|_| "读取目标进程身份失败")?)
    };
    identity(native(&process), pid)
}

// 返回 100ns 创建时间和真实可执行路径；长路径按 Windows 上限分配，不截断名称后继续注入。
fn identity(process: HANDLE, pid: u32) -> Result<ProcessCandidate, String> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    unsafe {
        GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|_| "读取目标创建时间失败")?;
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .map_err(|_| "读取目标可执行路径失败")?;
    }
    Ok(ProcessCandidate {
        pid,
        createdAt: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        executable: PathBuf::from(
            String::from_utf16(&path[..length as usize]).map_err(|_| "目标路径不是有效 UTF-16")?,
        ),
    })
}

// 返回进程实际机器类型；原生进程返回 nativeMachine，WOW64 返回 processMachine，覆盖 ARM64 模拟边界。
fn machine(process: HANDLE) -> Result<IMAGE_FILE_MACHINE, String> {
    let mut processMachine = IMAGE_FILE_MACHINE::default();
    let mut nativeMachine = IMAGE_FILE_MACHINE::default();
    unsafe {
        IsWow64Process2(process, &mut processMachine, Some(&mut nativeMachine))
            .map_err(|_| "读取目标体系架构失败")?;
    }
    Ok(if processMachine.0 == 0 {
        nativeMachine
    } else {
        processMachine
    })
}

// 模块表用于精确确认 DLL 是否加载以及系统入口所属模块，不用截断的线程退出码充当 64 位 HMODULE。
fn modules(pid: u32) -> Result<Vec<MODULEENTRY32W>, String> {
    let snapshot = unsafe {
        own(
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
                .map_err(|_| "枚举目标模块失败")?,
        )
    };
    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    let mut result = Vec::new();
    unsafe {
        Module32FirstW(native(&snapshot), &mut entry).map_err(|_| "目标模块列表不可读")?;
        loop {
            result.push(entry);
            match Module32NextW(native(&snapshot), &mut entry) {
                Ok(()) => {}
                Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
                Err(_) => return Err("目标模块枚举中断".into()),
            }
        }
    }
    Ok(result)
}

// Windows 路径只规范化扩展路径前缀、分隔符与大小写；不依赖两进程相同的加载基址。
fn normalizedPath(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .to_lowercase()
}

// 模块路径数组来自 ToolHelp；按 NUL 截断，不把固定缓冲区尾部包含进路径。
fn modulePath(entry: &MODULEENTRY32W) -> PathBuf {
    let length = entry
        .szExePath
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(entry.szExePath.len());
    PathBuf::from(String::from_utf16_lossy(&entry.szExePath[..length]))
}

// 若 GetProcAddress 转发到 KernelBase，按真实拥有模块计算 RVA，再映射到目标该模块，不复用本进程地址。
fn remoteLoader(pid: u32) -> Result<usize, String> {
    unsafe {
        let kernel = GetModuleHandleW(windows::core::w!("kernel32.dll"))
            .map_err(|_| "读取加载器模块失败")?;
        let loader =
            GetProcAddress(kernel, s!("LoadLibraryW")).ok_or("读取加载器入口失败")? as usize;
        let mut owner = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(loader as *const u16),
            &mut owner,
        )
        .map_err(|_| "解析加载器所属模块失败")?;
        let local = modules(std::process::id())?
            .into_iter()
            .find(|entry| entry.hModule == owner)
            .ok_or("加载器所属模块不在列表中")?;
        let path = normalizedPath(&modulePath(&local));
        let remote = modules(pid)?
            .into_iter()
            .find(|entry| normalizedPath(&modulePath(entry)) == path)
            .ok_or("目标缺少匹配的系统加载器模块")?;
        let offset = loader
            .checked_sub(owner.0 as usize)
            .filter(|offset| *offset < remote.modBaseSize as usize)
            .ok_or("加载器 RVA 超出目标模块")?;
        (remote.modBaseAddr as usize)
            .checked_add(offset)
            .ok_or_else(|| "目标加载器地址溢出".into())
    }
}

// 先启动清理消费者再创建远程线程；保留超时任务到线程实际结束，正常关闭服务也不释放仍被目标读取的参数。
fn cleanupQueue() -> Result<&'static mpsc::Sender<RemoteLoad>, String> {
    static queue: OnceLock<Result<mpsc::Sender<RemoteLoad>, String>> = OnceLock::new();
    queue
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<RemoteLoad>();
            std::thread::Builder::new()
                .name("observationLoadCleanup".into())
                .spawn(move || {
                    let mut waiting: Vec<RemoteLoad> = Vec::new();
                    loop {
                        // 空闲时阻塞等待，不为没有超时任务的正常运行保留周期轮询开销。
                        let received = if waiting.is_empty() {
                            receiver
                                .recv()
                                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                        } else {
                            receiver.recv_timeout(cleanupInterval)
                        };
                        match received {
                            Ok(load) => waiting.push(load),
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) if waiting.is_empty() => {
                                break
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                std::thread::sleep(cleanupInterval)
                            }
                        }
                        waiting.retain(|load| unsafe {
                            let pending = WaitForSingleObject(native(&load.thread), 0)
                                != WAIT_OBJECT_0
                                && WaitForSingleObject(native(&load.allocation.process), 0)
                                    != WAIT_OBJECT_0;
                            if !pending {
                                log::debug!(
                                    "已回收超时加载参数 pid={}",
                                    load.reservation.identity.0
                                );
                            }
                            pending
                        });
                    }
                })
                .map_err(|_| "启动加载参数清理线程失败".to_string())?;
            Ok(sender)
        })
        .as_ref()
        .map_err(Clone::clone)
}

// 等待 DLL 自身初始化完成的事件；LoadLibrary 成功但配置缺失/初始化失败均不会返回就绪。
fn waitReady(process: HANDLE, pid: u32, module: &Path) -> Result<(), String> {
    let name = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(pid, module));
    let started = Instant::now();
    loop {
        if unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0 {
            return Err("目标在观测模块就绪前退出".into());
        }
        match unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
            Ok(event) => {
                let event = unsafe { own(event) };
                if unsafe { WaitForSingleObject(native(&event), 0) } == WAIT_OBJECT_0 {
                    return Ok(());
                }
            }
            Err(error) if error.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {}
            Err(_) => return Err("读取观测就绪事件失败".into()),
        }
        if started.elapsed() >= readyTimeout {
            return Err("观测 DLL 已加载但未就绪".into());
        }
        std::thread::sleep(cleanupInterval);
    }
}

// 验证实例和架构后执行一次加载；任何系统调用失败返回诊断，重复加载先检查模块表而不增加引用计数。
pub(super) fn inject(expected: &ProcessCandidate, dll: &Path) -> Result<(), String> {
    let dll = std::fs::canonicalize(dll).map_err(|_| "观测 DLL 文件不可读")?;
    if !dll.is_file() {
        return Err("观测 DLL 路径不是文件".into());
    }
    let cleanup = cleanupQueue()?;
    let access = PROCESS_QUERY_INFORMATION
        | PROCESS_CREATE_THREAD
        | PROCESS_VM_OPERATION
        | PROCESS_VM_WRITE
        | PROCESS_VM_READ
        | PROCESS_SYNCHRONIZE;
    let process = unsafe {
        own(OpenProcess(access, false, expected.pid).map_err(|_| "打开观测目标失败")?)
    };
    let actual = identity(native(&process), expected.pid)?;
    if actual.createdAt != expected.createdAt
        || normalizedPath(&actual.executable) != normalizedPath(&expected.executable)
    {
        return Err("目标进程实例已变化，请重新扫描".into());
    }
    if machine(native(&process))? != machine(unsafe { GetCurrentProcess() })? {
        return Err("观测目标与宿主体系架构不一致".into());
    }
    let reservation = Reservation::acquire(expected)?;
    if modules(expected.pid)?
        .iter()
        .any(|entry| normalizedPath(&modulePath(entry)) == normalizedPath(&dll))
    {
        return waitReady(native(&process), expected.pid, &dll);
    }
    let loader = remoteLoader(expected.pid)?;
    let path: Vec<u16> = dll.as_os_str().encode_wide().chain(Some(0)).collect();
    let length = path.len().checked_mul(2).ok_or("DLL 路径长度溢出")?;
    let remote = unsafe {
        VirtualAllocEx(
            native(&process),
            None,
            length,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    if remote.is_null() {
        return Err("分配观测加载参数失败".into());
    }
    let allocation = RemoteAllocation {
        process,
        address: remote as usize,
    };
    let mut written = 0;
    unsafe {
        WriteProcessMemory(
            native(&allocation.process),
            remote,
            path.as_ptr().cast(),
            length,
            Some(&mut written),
        )
        .map_err(|_| "写入观测加载参数失败")?;
    }
    if written != length {
        return Err("观测加载参数写入不完整".into());
    }
    let thread = unsafe {
        let entry =
            std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(loader);
        own(CreateRemoteThread(
            native(&allocation.process),
            None,
            0,
            Some(entry),
            Some(remote),
            0,
            None,
        )
        .map_err(|_| "创建观测加载线程失败")?)
    };
    let load = RemoteLoad {
        thread,
        allocation,
        reservation,
    };
    let completed = unsafe { WaitForSingleObject(native(&load.thread), loadTimeoutMs) };
    if completed != WAIT_OBJECT_0 {
        // 消费者在首次加载之前已创建；灾难性消费者退出时宁可保留参数到目标退出，也不造成远程悬空指针。
        if let Err(undelivered) = cleanup.send(load) {
            std::mem::forget(undelivered.0);
            return Err("加载清理线程停止，远程参数保留到目标退出".into());
        }
        return Err(if completed == WAIT_TIMEOUT {
            "观测 DLL 加载超时，仍在等待安全回收"
        } else {
            "等待观测加载线程失败，仍在等待安全回收"
        }
        .into());
    }
    if !modules(expected.pid)?
        .iter()
        .any(|entry| normalizedPath(&modulePath(entry)) == normalizedPath(&dll))
    {
        return Err("目标没有加载观测 DLL".into());
    }
    waitReady(native(&load.allocation.process), expected.pid, &dll)
}

#[cfg(test)]
#[path = "../../tests/observation/nativeInjectionTests.rs"]
mod tests;
