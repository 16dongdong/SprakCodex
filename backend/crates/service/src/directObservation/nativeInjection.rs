//! Windows x64 内存部署事务：宿主完成 PE 映射，目标只运行受控初始化入口。
use super::processInjector::ProcessCandidate;
use cpcommon::deploymentLifecycle::DeploymentRecord;
use pelite::pe64::{image::*, imports::Import, Pe, PeFile, PeObject};
use std::{
    collections::HashSet,
    ffi::c_void,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use windows::{
    core::{s, PCSTR, PWSTR},
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, FILETIME, HANDLE, HMODULE, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        System::{
            Diagnostics::{
                Debug::{FlushInstructionCache, ReadProcessMemory, WriteProcessMemory},
                ToolHelp::{
                    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W,
                    TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
                },
            },
            LibraryLoader::{
                GetModuleHandleExW, GetModuleHandleW, GetProcAddress, LoadLibraryA,
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
            Memory::{
                VirtualAllocEx, VirtualFreeEx, VirtualProtectEx, MEM_COMMIT, MEM_RELEASE,
                MEM_RESERVE, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
                PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
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
const runtimeFunctionBytes: u32 = 12;
static pendingLoads: Mutex<Option<HashSet<(u32, u64)>>> = Mutex::new(None);

// 纯位置无关引导只调用上下文内的远程地址，可直接复制到目标代码页。
std::arch::global_asm!(
    r#"
    .text
    .p2align 4
    .global observationMemoryLoaderStart
observationMemoryLoaderStart:
    push rbx
    sub rsp, 32
    mov rbx, rcx
    mov dword ptr [rbx + 52], 1
    cmp dword ptr [rbx + 24], 0
    je 2f
    mov rcx, qword ptr [rbx + 16]
    mov edx, dword ptr [rbx + 24]
    mov r8, qword ptr [rbx]
    call qword ptr [rbx + 32]
    test eax, eax
    je 7f
2:
    mov dword ptr [rbx + 52], 2
    mov rcx, qword ptr [rbx]
    mov edx, 1
    xor r8d, r8d
    call qword ptr [rbx + 8]
    test eax, eax
    je 6f
    mov dword ptr [rbx + 52], 3
    lea rcx, [rbx + 52]
    call qword ptr [rbx + 40]
    test eax, eax
    je .LinitializeRejected
    mov dword ptr [rbx + 48], 1
    mov dword ptr [rbx + 52], 4
    xor eax, eax
    jmp 9f
6:
    mov dword ptr [rbx + 48], 2
    jmp 9f
7:
    mov dword ptr [rbx + 48], 3
    jmp 9f
.LinitializeRejected:
    mov dword ptr [rbx + 48], 4
9:
    add rsp, 32
    pop rbx
    ret
    .global observationMemoryLoaderEnd
observationMemoryLoaderEnd:
"#
);

extern "C" {
    static observationMemoryLoaderStart: u8;
    static observationMemoryLoaderEnd: u8;
}

// 预留绑定完整进程实例；重复扫描不会建立并行部署事务。
struct Reservation {
    identity: (u32, u64),
}
impl Reservation {
    // 同时限制活跃和延迟回收任务；失败发生在任何远程写入之前。
    fn acquire(candidate: &ProcessCandidate) -> Result<Self, String> {
        let mut guard = pendingLoads.lock().map_err(|_| "部署任务状态锁损坏")?;
        let active = guard.get_or_insert_with(HashSet::new);
        let identity = (candidate.pid, candidate.createdAt);
        if active.len() >= maxPendingLoads || !active.insert(identity) {
            return Err("目标部署尚未完成或等待队列已满".into());
        }
        Ok(Self { identity })
    }
}
impl Drop for Reservation {
    // 所有完成路径都解除预留；析构不传播状态锁错误。
    fn drop(&mut self) {
        if let Ok(mut guard) = pendingLoads.lock() {
            if let Some(active) = guard.as_mut() {
                active.remove(&self.identity);
            }
        }
    }
}

// 远程事务统一拥有句柄与分配；只有 DllMain 成功后才保留映像和 TLS 模板。
struct RemoteLoad {
    process: OwnedHandle,
    thread: OwnedHandle,
    transient: Vec<usize>,
    persistent: Vec<usize>,
    contextAddress: usize,
    committed: bool,
    record: DeploymentRecord,
    reservation: Reservation,
}

// 构建阶段的分配先由本地守卫拥有；任何解析或写入错误都会回收已经创建的远程区域。
struct AllocationSet {
    process: HANDLE,
    addresses: Vec<usize>,
}
impl AllocationSet {
    // 分配成功后立即登记，避免后续步骤用问号返回时泄漏目标进程内存。
    fn allocate(
        &mut self,
        length: usize,
        protection: windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS,
    ) -> Result<usize, String> {
        let address = allocate(self.process, length, protection)?;
        self.addresses.push(address);
        Ok(address)
    }

    // 所有权转交 RemoteLoad 后清空本守卫，确保每个区域只有一个释放方。
    fn release(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.addresses)
    }
}
impl Drop for AllocationSet {
    // 仅回收当前守卫仍拥有的构建期区域；目标退出时系统调用失败不覆盖原始错误。
    fn drop(&mut self) {
        for &address in &self.addresses {
            unsafe {
                let _ = VirtualFreeEx(self.process, address as *mut c_void, 0, MEM_RELEASE);
            }
        }
    }
}
impl RemoteLoad {
    // 引导完成后读取固定结果；短读和具体阶段码都进入宿主诊断，只有完整成功才保留常驻映像。
    fn commit(&mut self) -> Result<(), String> {
        let mut context = LoaderContext::default();
        let mut read = 0;
        let result = unsafe {
            ReadProcessMemory(
                native(&self.process),
                self.contextAddress as *const c_void,
                (&mut context as *mut LoaderContext).cast(),
                std::mem::size_of::<LoaderContext>(),
                Some(&mut read),
            )
        };
        self.committed =
            result.is_ok() && read == std::mem::size_of::<LoaderContext>() && context.result == 1;
        if result.is_err() || read != std::mem::size_of::<LoaderContext>() {
            return Err("读取内存部署结果失败".into());
        }
        self.committed.then_some(()).ok_or_else(|| {
            format!(
                "观测内存映像初始化失败：阶段={}，结果={}",
                context.stage, context.result
            )
        })
    }
}
impl Drop for RemoteLoad {
    // 引导页始终回收；未提交事务连同半初始化映像一起回收。
    fn drop(&mut self) {
        let process = native(&self.process);
        for &address in &self.transient {
            unsafe {
                if VirtualFreeEx(process, address as *mut c_void, 0, MEM_RELEASE).is_err() {
                    log::error!("回收内存部署引导页失败 pid={}", self.reservation.identity.0);
                }
            }
        }
        if !self.committed {
            for &address in &self.persistent {
                unsafe {
                    let _ = VirtualFreeEx(process, address as *mut c_void, 0, MEM_RELEASE);
                }
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LoaderContext {
    imageBase: u64,
    entryPoint: u64,
    functionTable: u64,
    functionCount: u32,
    reserved: u32,
    rtlAddFunctionTable: u64,
    initializer: u64,
    result: u32,
    stage: u32,
}

// API 成功返回的 Windows 句柄立即移交 RAII。
unsafe fn own(value: HANDLE) -> OwnedHandle {
    OwnedHandle::from_raw_handle(value.0)
}
// 只借用句柄，不转移关闭责任。
fn native(value: &OwnedHandle) -> HANDLE {
    HANDLE(value.as_raw_handle())
}

// 读取 PID 对应的真实创建时间与完整可执行路径。
pub(super) fn candidate(pid: u32) -> Result<ProcessCandidate, String> {
    let process = unsafe {
        own(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|_| "读取目标进程身份失败")?)
    };
    identity(native(&process), pid)
}

// 身份字段来自同一进程句柄，避免 PID 复用造成跨实例写入。
fn identity(process: HANDLE, pid: u32) -> Result<ProcessCandidate, String> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
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

// WOW64 与原生机器类型分开读取，部署只接受和宿主一致的架构。
fn machine(process: HANDLE) -> Result<IMAGE_FILE_MACHINE, String> {
    let (mut processMachine, mut nativeMachine) =
        (IMAGE_FILE_MACHINE::default(), IMAGE_FILE_MACHINE::default());
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

// 每次地址换算重新枚举模块，不复用跨进程或跨启动基址。
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

// 路径身份只统一 Windows 扩展前缀、分隔符和大小写。
fn normalizedPath(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .to_lowercase()
}
// ToolHelp 路径按首个 NUL 截断。
fn modulePath(entry: &MODULEENTRY32W) -> PathBuf {
    let length = entry
        .szExePath
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(entry.szExePath.len());
    PathBuf::from(String::from_utf16_lossy(&entry.szExePath[..length]))
}

// 先定位本地函数真实所属模块，再用 RVA 换算目标地址，兼容转发导出和 API Set。
fn remoteFunction(pid: u32, localAddress: usize) -> Result<usize, String> {
    let mut owner = HMODULE::default();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(localAddress as *const u16),
            &mut owner,
        )
        .map_err(|_| "解析函数所属模块失败")?;
    }
    let local = modules(std::process::id())?
        .into_iter()
        .find(|entry| entry.hModule == owner)
        .ok_or("函数所属模块不在本地列表中")?;
    let path = normalizedPath(&modulePath(&local));
    let remote = modules(pid)?
        .into_iter()
        .find(|entry| normalizedPath(&modulePath(entry)) == path)
        .ok_or("目标缺少函数所属系统模块")?;
    let offset = localAddress
        .checked_sub(owner.0 as usize)
        .filter(|offset| *offset < remote.modBaseSize as usize)
        .ok_or("函数 RVA 超出所属模块")?;
    (remote.modBaseAddr as usize)
        .checked_add(offset)
        .ok_or_else(|| "远程函数地址溢出".into())
}

// 分配指定权限的完整远程区域；空地址直接转换为明确错误。
fn allocate(
    process: HANDLE,
    length: usize,
    protection: windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS,
) -> Result<usize, String> {
    let address =
        unsafe { VirtualAllocEx(process, None, length, MEM_COMMIT | MEM_RESERVE, protection) };
    (!address.is_null())
        .then_some(address as usize)
        .ok_or_else(|| "分配目标进程内存失败".into())
}

// 跨进程写入必须完整，不接受短写后继续初始化。
fn writeRemote(process: HANDLE, address: usize, bytes: &[u8]) -> Result<(), String> {
    let mut written = 0;
    unsafe {
        WriteProcessMemory(
            process,
            address as *mut c_void,
            bytes.as_ptr().cast(),
            bytes.len(),
            Some(&mut written),
        )
        .map_err(|_| "写入目标进程内存失败")?;
    }
    (written == bytes.len())
        .then_some(())
        .ok_or_else(|| "目标进程内存写入不完整".into())
}

// 仅系统依赖交给 Windows loader；内嵌业务映像不会传递磁盘路径。
fn loadDependency(process: HANDLE, pid: u32, name: &[u8]) -> Result<(), String> {
    let kernel = unsafe { GetModuleHandleW(windows::core::w!("kernel32.dll")) }
        .map_err(|_| "读取本地加载器模块失败")?;
    let localLoader = unsafe { GetProcAddress(kernel, s!("LoadLibraryA")) }
        .ok_or("读取依赖加载入口失败")? as usize;
    let loader = remoteFunction(pid, localLoader)?;
    let argument = allocate(process, name.len(), PAGE_READWRITE)?;
    let mut completed = false;
    let result = (|| {
        writeRemote(process, argument, name)?;
        let entry = unsafe {
            std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(loader)
        };
        let thread = unsafe {
            own(CreateRemoteThread(
                process,
                None,
                0,
                Some(entry),
                Some(argument as *const c_void),
                0,
                None,
            )
            .map_err(|_| "创建依赖加载线程失败")?)
        };
        completed = unsafe { WaitForSingleObject(native(&thread), loadTimeoutMs) } == WAIT_OBJECT_0;
        completed
            .then_some(())
            .ok_or_else(|| "等待系统依赖加载超时".into())
    })();
    // 超时后远程线程仍可能读取参数；宁可把极小字符串留到目标退出，也不制造悬空指针。
    if completed {
        unsafe {
            let _ = VirtualFreeEx(process, argument as *mut c_void, 0, MEM_RELEASE);
        }
    }
    result
}

// 内嵌字节复制到自然对齐存储，满足 PE 解析器的四字节对齐契约。
pub(super) fn alignedImage(image: &[u8]) -> Vec<u64> {
    let mut words = vec![0u64; image.len().div_ceil(8)];
    unsafe {
        std::ptr::copy_nonoverlapping(image.as_ptr(), words.as_mut_ptr().cast(), image.len());
    }
    words
}

// 文件布局显式展开为节对齐映像；源和目标范围都先校验，不依赖快捷转换的节尾假设。
fn mapImage(file: PeFile<'_>) -> Result<Vec<u8>, String> {
    let source = file.image();
    let mut mapped = vec![0u8; file.optional_header().SizeOfImage as usize];
    let headerBytes = file.optional_header().SizeOfHeaders as usize;
    let headers = source.get(..headerBytes).ok_or("PE 头部源范围越界")?;
    mapped
        .get_mut(..headerBytes)
        .ok_or("PE 头部目标范围越界")?
        .copy_from_slice(headers);
    for section in file.section_headers() {
        let sourceStart = section.PointerToRawData as usize;
        let sourceEnd = sourceStart
            .checked_add(section.SizeOfRawData as usize)
            .ok_or("PE 节源范围溢出")?;
        let targetStart = section.VirtualAddress as usize;
        let targetEnd = targetStart
            .checked_add(section.SizeOfRawData as usize)
            .ok_or("PE 节目标范围溢出")?;
        let bytes = source
            .get(sourceStart..sourceEnd)
            .ok_or("PE 节源范围越界")?;
        mapped
            .get_mut(targetStart..targetEnd)
            .ok_or("PE 节目标范围越界")?
            .copy_from_slice(bytes);
    }
    Ok(mapped)
}

// x64 载荷只接受 DIR64 重定位；未知类型说明部署器与产物不兼容。
fn applyRelocations(file: PeFile<'_>, mapped: &mut [u8], remoteBase: u64) -> Result<(), String> {
    let delta = remoteBase.wrapping_sub(file.optional_header().ImageBase);
    let mut failure = None;
    file.base_relocs()
        .map_err(|_| "读取 PE 重定位表失败")?
        .for_each(|rva, kind| {
            if failure.is_some() || kind == IMAGE_REL_BASED_ABSOLUTE {
                return;
            }
            if kind != IMAGE_REL_BASED_DIR64 {
                failure = Some("PE 含不支持的重定位类型".to_string());
                return;
            }
            let offset = rva as usize;
            let Some(bytes) = mapped.get_mut(offset..offset + 8) else {
                failure = Some("PE 重定位地址越界".to_string());
                return;
            };
            let value = u64::from_le_bytes(bytes.try_into().unwrap()).wrapping_add(delta);
            bytes.copy_from_slice(&value.to_le_bytes());
        });
    failure.map_or(Ok(()), Err)
}

// 导入先加载依赖，再按真实所有者模块 RVA 写入目标 IAT。
fn resolveImports(
    file: PeFile<'_>,
    mapped: &mut [u8],
    process: HANDLE,
    pid: u32,
) -> Result<(), String> {
    for descriptor in file.imports().map_err(|_| "读取 PE 导入表失败")? {
        let name = descriptor.dll_name().map_err(|_| "读取导入模块名称失败")?;
        loadDependency(process, pid, name.c_str())?;
        let local = unsafe { LoadLibraryA(PCSTR(name.c_str().as_ptr())) }
            .map_err(|_| "本地解析导入模块失败")?;
        for (index, import) in descriptor
            .int()
            .map_err(|_| "读取导入名称表失败")?
            .enumerate()
        {
            let symbol = match import.map_err(|_| "读取导入符号失败")? {
                Import::ByName { name, .. } => PCSTR(name.c_str().as_ptr()),
                Import::ByOrdinal { ord } => PCSTR(ord as usize as *const u8),
            };
            let localAddress =
                unsafe { GetProcAddress(local, symbol) }.ok_or("解析导入符号失败")? as usize;
            let remoteAddress = remoteFunction(pid, localAddress)? as u64;
            let offset = descriptor.image().FirstThunk as usize + index * 8;
            mapped
                .get_mut(offset..offset + 8)
                .ok_or("导入地址表越界")?
                .copy_from_slice(&remoteAddress.to_le_bytes());
        }
    }
    Ok(())
}

// PE 节属性映射为最小页面权限。
fn sectionProtection(
    characteristics: u32,
) -> windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS {
    match (
        characteristics & IMAGE_SCN_MEM_EXECUTE != 0,
        characteristics & IMAGE_SCN_MEM_READ != 0,
        characteristics & IMAGE_SCN_MEM_WRITE != 0,
    ) {
        (false, false, false) => PAGE_NOACCESS,
        (false, true, false) => PAGE_READONLY,
        (false, _, true) => PAGE_READWRITE,
        (true, false, false) => PAGE_EXECUTE,
        (true, true, false) => PAGE_EXECUTE_READ,
        (true, _, true) => PAGE_EXECUTE_READWRITE,
    }
}

// 完成写入后收紧头部和各节权限，并刷新目标指令缓存。
fn protectImage(file: PeFile<'_>, process: HANDLE, remoteBase: usize) -> Result<(), String> {
    let mut previous = Default::default();
    unsafe {
        VirtualProtectEx(
            process,
            remoteBase as *const c_void,
            file.optional_header().SizeOfHeaders as usize,
            PAGE_READONLY,
            &mut previous,
        )
        .map_err(|_| "保护 PE 头部失败")?;
    }
    for section in file.section_headers() {
        let length = section.VirtualSize.max(section.SizeOfRawData) as usize;
        if length == 0 {
            continue;
        }
        unsafe {
            VirtualProtectEx(
                process,
                (remoteBase + section.VirtualAddress as usize) as *const c_void,
                length,
                sectionProtection(section.Characteristics),
                &mut previous,
            )
            .map_err(|_| "保护 PE 区段失败")?;
        }
    }
    unsafe { FlushInstructionCache(process, Some(remoteBase as *const c_void), 0) }
        .map_err(|_| "刷新目标指令缓存失败".to_string())
}

// 导出必须是映像内的直接符号；转发导出和缺失名称都不参与私有生命周期 ABI。
fn exportRva(file: PeFile<'_>, name: &[u8]) -> Result<u32, String> {
    file.exports()
        .and_then(|exports| exports.by())
        .and_then(|exports| exports.name(name))
        .ok()
        .and_then(|export| export.symbol())
        .ok_or_else(|| format!("内嵌观测载荷缺少 {} 入口", String::from_utf8_lossy(name)))
}

// 异常表及入口地址一次性固化到远程引导上下文。
fn loaderContext(file: PeFile<'_>, pid: u32, remoteBase: usize) -> Result<LoaderContext, String> {
    let remoteSystem = |module: HMODULE, name: PCSTR| {
        unsafe { GetProcAddress(module, name) }
            .ok_or_else(|| "读取系统初始化入口失败".to_string())
            .and_then(|address| remoteFunction(pid, address as usize))
            .map(|address| address as u64)
    };
    let ntdll = unsafe { GetModuleHandleW(windows::core::w!("ntdll.dll")) }
        .map_err(|_| "读取异常系统模块失败")?;
    let initializerRva = exportRva(file, b"observationInitialize")?;
    let mut context = LoaderContext {
        imageBase: remoteBase as u64,
        entryPoint: (remoteBase + file.optional_header().AddressOfEntryPoint as usize) as u64,
        rtlAddFunctionTable: remoteSystem(ntdll, s!("RtlAddFunctionTable"))?,
        initializer: (remoteBase + initializerRva as usize) as u64,
        ..Default::default()
    };
    if let Some(exception) = file.data_directory().get(IMAGE_DIRECTORY_ENTRY_EXCEPTION) {
        if exception.VirtualAddress != 0 && exception.Size != 0 {
            if exception.Size % runtimeFunctionBytes != 0 {
                return Err("异常函数表长度无效".into());
            }
            context.functionTable = (remoteBase + exception.VirtualAddress as usize) as u64;
            context.functionCount = exception.Size / runtimeFunctionBytes;
        }
    }
    Ok(context)
}

// 起止符号限定单段位置无关机器码，异常范围拒绝执行。
fn loaderCode() -> Result<&'static [u8], String> {
    let start = unsafe { &observationMemoryLoaderStart as *const u8 as usize };
    let end = unsafe { &observationMemoryLoaderEnd as *const u8 as usize };
    let length = end
        .checked_sub(start)
        .filter(|length| *length > 0 && *length < 4096)
        .ok_or("内存部署引导代码范围无效")?;
    Ok(unsafe { std::slice::from_raw_parts(start as *const u8, length) })
}

// 超时事务由单一消费者持有到线程终止，避免释放仍在执行的代码或上下文。
fn cleanupQueue() -> Result<&'static mpsc::Sender<RemoteLoad>, String> {
    static queue: OnceLock<Result<mpsc::Sender<RemoteLoad>, String>> = OnceLock::new();
    queue
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<RemoteLoad>();
            std::thread::Builder::new()
                .name("observationMemoryCleanup".into())
                .spawn(move || {
                    let mut waiting: Vec<RemoteLoad> = Vec::new();
                    loop {
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
                        waiting.retain_mut(|load| unsafe {
                            let pending = WaitForSingleObject(native(&load.thread), 0)
                                != WAIT_OBJECT_0
                                && WaitForSingleObject(native(&load.process), 0) != WAIT_OBJECT_0;
                            if !pending {
                                if let Err(error) = load.commit() {
                                    log::error!(
                                        "延迟回收内存部署事务失败 pid={}：{error}",
                                        load.reservation.identity.0
                                    );
                                } else {
                                    if let Err(error) = super::processInjector::recordDeployment(
                                        load.record.clone(),
                                    ) {
                                        log::error!("同步延迟部署记录失败：{error}");
                                        if let Err(unloadError) =
                                            cpcommon::deploymentLifecycle::unload(&load.record)
                                        {
                                            log::error!("回滚未跟踪的延迟部署失败：{unloadError}");
                                        }
                                    }
                                }
                            }
                            pending
                        });
                    }
                })
                .map_err(|_| "启动内存部署清理线程失败".to_string())?;
            Ok(sender)
        })
        .as_ref()
        .map_err(Clone::clone)
}

// 等待版本化事件，并同时观察目标进程退出。
fn waitEvent(process: HANDLE, name: String, timeout: Duration) -> Result<(), String> {
    let name = windows::core::HSTRING::from(name);
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
            Err(_) => return Err("读取观测模块事件失败".into()),
        }
        if started.elapsed() >= timeout {
            return Err("观测内存模块已加载但未就绪".into());
        }
        std::thread::sleep(cleanupInterval);
    }
}

// 加载事件存在时复用当前内存映像，不重复映射。
fn loaded(pid: u32) -> bool {
    let name = windows::core::HSTRING::from(cpcommon::hook_ready::loaded_event_name(
        pid,
        cpcommon::relayContract::deploymentIdentity,
    ));
    unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) }
        .map(|event| unsafe { own(event) })
        .is_ok_and(|event| unsafe { WaitForSingleObject(native(&event), 0) } == WAIT_OBJECT_0)
}

// 验证实例和架构后，从内嵌字节完成一次映射并等待模块自身就绪。
pub(super) fn inject(
    expected: &ProcessCandidate,
    image: &[u8],
) -> Result<Option<DeploymentRecord>, String> {
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
    if loaded(expected.pid) {
        waitEvent(
            native(&process),
            cpcommon::hook_ready::event_name(
                expected.pid,
                cpcommon::relayContract::deploymentIdentity,
            ),
            readyTimeout,
        )?;
        return Ok(None);
    }
    let reservation = Reservation::acquire(expected)?;
    let aligned = alignedImage(image);
    let alignedBytes =
        unsafe { std::slice::from_raw_parts(aligned.as_ptr().cast::<u8>(), image.len()) };
    let file = PeFile::from_bytes(alignedBytes).map_err(|_| "内嵌观测载荷不是有效 PE32+ 映像")?;
    if file.file_header().Machine != IMAGE_FILE_MACHINE_AMD64
        || file.file_header().Characteristics & IMAGE_FILE_DLL == 0
    {
        return Err("内嵌观测载荷不是 Windows x64 DLL".into());
    }
    let mut mapped = mapImage(file)?;
    let mut persistent = AllocationSet {
        process: native(&process),
        addresses: Vec::new(),
    };
    let imageAddress =
        persistent.allocate(file.optional_header().SizeOfImage as usize, PAGE_READWRITE)?;
    applyRelocations(file, &mut mapped, imageAddress as u64)?;
    resolveImports(file, &mut mapped, native(&process), expected.pid)?;
    writeRemote(native(&process), imageAddress, &mapped)?;
    let context = loaderContext(file, expected.pid, imageAddress)?;
    let record = DeploymentRecord {
        processId: expected.pid,
        createdAt: expected.createdAt,
        imageBase: imageAddress,
        imageSize: file.optional_header().SizeOfImage as usize,
        entryPoint: context.entryPoint as usize,
        shutdownEntry: imageAddress + exportRva(file, b"observationShutdown")? as usize,
        functionTable: context.functionTable as usize,
    };
    record.validate().map_err(str::to_owned)?;
    protectImage(file, native(&process), imageAddress)?;

    let code = loaderCode()?;
    let mut transient = AllocationSet {
        process: native(&process),
        addresses: Vec::new(),
    };
    let codeAddress = transient.allocate(code.len(), PAGE_EXECUTE_READWRITE)?;
    writeRemote(native(&process), codeAddress, code)?;
    unsafe {
        FlushInstructionCache(
            native(&process),
            Some(codeAddress as *const c_void),
            code.len(),
        )
    }
    .map_err(|_| "刷新内存部署引导代码失败")?;
    let contextAddress =
        transient.allocate(std::mem::size_of::<LoaderContext>(), PAGE_READWRITE)?;
    let contextBytes = unsafe {
        std::slice::from_raw_parts(
            (&context as *const LoaderContext).cast::<u8>(),
            std::mem::size_of::<LoaderContext>(),
        )
    };
    writeRemote(native(&process), contextAddress, contextBytes)?;
    let entry = unsafe {
        std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(codeAddress)
    };
    let thread = unsafe {
        own(CreateRemoteThread(
            native(&process),
            None,
            0,
            Some(entry),
            Some(contextAddress as *const c_void),
            0,
            None,
        )
        .map_err(|_| "创建内存部署引导线程失败")?)
    };
    let mut load = RemoteLoad {
        process,
        thread,
        transient: transient.release(),
        persistent: persistent.release(),
        contextAddress,
        committed: false,
        record: record.clone(),
        reservation,
    };
    let completed = unsafe { WaitForSingleObject(native(&load.thread), loadTimeoutMs) };
    if completed != WAIT_OBJECT_0 {
        if let Err(undelivered) = cleanup.send(load) {
            std::mem::forget(undelivered.0);
            return Err("部署清理线程停止，远程内存保留到目标退出".into());
        }
        return Err(if completed == WAIT_TIMEOUT {
            "观测内存部署超时，仍在等待安全回收"
        } else {
            "等待观测内存部署线程失败，仍在等待安全回收"
        }
        .into());
    }
    load.commit()?;
    if let Err(error) = super::processInjector::recordDeployment(record.clone()) {
        cpcommon::deploymentLifecycle::unload(&record)
            .map_err(|unload| format!("{error}；回滚未跟踪的观测映像失败：{unload}"))?;
        return Err(error);
    }
    waitEvent(
        native(&load.process),
        cpcommon::hook_ready::event_name(expected.pid, cpcommon::relayContract::deploymentIdentity),
        readyTimeout,
    )?;
    Ok(Some(record))
}

#[cfg(test)]
#[path = "../../tests/observation/nativeInjectionTests.rs"]
mod tests;
