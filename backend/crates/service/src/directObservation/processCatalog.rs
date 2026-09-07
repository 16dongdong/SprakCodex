//! Windows 只读轻量进程目录：新系统使用基本目录，旧系统按信息类能力使用 Toolhelp，不采集资源统计或环境。
use super::processInjector::isTargetWideName;
use std::{
    ffi::c_void,
    os::windows::io::{FromRawHandle, OwnedHandle},
    sync::atomic::{AtomicBool, Ordering},
};
use windows::Win32::{
    Foundation::{ERROR_NO_MORE_FILES, HANDLE},
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};

const basicProcessInformation: u32 = 252;
const statusInvalidInfoClass: i32 = 0xc0000003u32 as i32;
const statusNotImplemented: i32 = 0xc0000002u32 as i32;
const statusLengthMismatch: i32 = 0xc0000004u32 as i32;
const initialBytes: usize = 128 * 1024;
const maxBytes: usize = 16 * 1024 * 1024;
static legacyCatalog: AtomicBool = AtomicBool::new(false);

#[link(name = "ntdll")]
extern "system" {
    // 调用方拥有输出内存，长度单位均为字节；保留 NTSTATUS 以区分版本能力、扩容和实际读取失败。
    #[link_name = "NtQuerySystemInformation"]
    fn querySystemInformation(
        class: u32,
        buffer: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

// 与公开 UNICODE_STRING ABI 一致；Buffer 只在本次查询分配范围内读取，不解引用任意返回地址。
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeName {
    length: u16,
    maximumLength: u16,
    buffer: *const u16,
}

// SYSTEM_BASICPROCESS_INFORMATION 的布局包含父 PID 和序号；加载前仍以真实进程句柄的创建时间二次核对。
#[repr(C)]
#[derive(Clone, Copy)]
struct BasicProcess {
    nextOffset: u32,
    pid: usize,
    parentPid: usize,
    sequence: u64,
    name: UnicodeName,
}

// 只有明确信息类不支持才选择旧版 API；权限、内存和格式错误向调用方返回，不伪装成空目录。
pub(super) fn targetPids() -> Result<Vec<u32>, String> {
    if !legacyCatalog.load(Ordering::Relaxed) {
        let mut allocation = vec![0u64; initialBytes / 8];
        for _ in 0..8 {
            let mut returned = 0;
            let status = unsafe {
                querySystemInformation(
                    basicProcessInformation,
                    allocation.as_mut_ptr().cast(),
                    (allocation.len() * 8) as u32,
                    &mut returned,
                )
            };
            if matches!(status, statusInvalidInfoClass | statusNotImplemented) {
                legacyCatalog.store(true, Ordering::Relaxed);
                break;
            }
            if status == statusLengthMismatch {
                let required = (returned as usize).max(allocation.len() * 16);
                if required > maxBytes {
                    return Err("基本进程目录超过大小上限".into());
                }
                allocation.resize(required.div_ceil(8), 0);
                continue;
            }
            if status < 0 {
                return Err(format!("读取基本进程目录失败：0x{:08x}", status as u32));
            }
            if returned as usize > allocation.len() * 8 {
                return Err("基本进程目录返回长度越界".into());
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(allocation.as_ptr().cast::<u8>(), returned as usize)
            };
            return parseBasic(bytes);
        }
        if !legacyCatalog.load(Ordering::Relaxed) {
            return Err("基本进程目录持续增长，读取未完成".into());
        }
    }
    toolhelpPids()
}

// 在已经完成的快照内校验链偏移与 Unicode 范围，仅输出匹配文件名的 PID；格式错误不部分返回。
fn parseBasic(bytes: &[u8]) -> Result<Vec<u32>, String> {
    let base = bytes.as_ptr() as usize;
    let limit = base.checked_add(bytes.len()).ok_or("进程目录地址溢出")?;
    let mut cursor = 0usize;
    let mut selected = Vec::new();
    loop {
        let headerEnd = cursor
            .checked_add(std::mem::size_of::<BasicProcess>())
            .ok_or("进程条目偏移溢出")?;
        if headerEnd > bytes.len() {
            return Err("进程条目截断".into());
        }
        let entry =
            unsafe { std::ptr::read_unaligned(bytes.as_ptr().add(cursor).cast::<BasicProcess>()) };
        if entry.name.length > 0 {
            let address = entry.name.buffer as usize;
            let length = entry.name.length as usize;
            if length % 2 != 0
                || entry.name.length > entry.name.maximumLength
                || address % 2 != 0
                || address < base
                || address.checked_add(length).is_none_or(|end| end > limit)
            {
                return Err("进程名称范围无效".into());
            }
            let name = unsafe { std::slice::from_raw_parts(entry.name.buffer, length / 2) };
            if isTargetWideName(name) {
                selected.push(u32::try_from(entry.pid).map_err(|_| "进程 PID 越界")?);
            }
        }
        if entry.nextOffset == 0 {
            return Ok(selected);
        }
        if (entry.nextOffset as usize) < std::mem::size_of::<BasicProcess>() {
            return Err("进程链条未向前推进".into());
        }
        cursor = cursor
            .checked_add(entry.nextOffset as usize)
            .ok_or("进程链条溢出")?;
    }
}

// 旧系统仅枚举进程名和 PID，不获取全机资源统计；每次快照句柄由 OwnedHandle 确定性释放。
fn toolhelpPids() -> Result<Vec<u32>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|_| "创建进程目录快照失败")?;
    let _owned = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut result = unsafe { Process32FirstW(HANDLE(snapshot.0), &mut entry) };
    let mut selected = Vec::new();
    loop {
        if let Err(error) = result {
            return if error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
                Ok(selected)
            } else {
                Err("枚举进程目录失败".into())
            };
        }
        let length = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        if isTargetWideName(&entry.szExeFile[..length]) {
            selected.push(entry.th32ProcessID);
        }
        result = unsafe { Process32NextW(snapshot, &mut entry) };
    }
}

#[cfg(test)]
#[path = "../../tests/observation/processCatalogTests.rs"]
mod tests;
