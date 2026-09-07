//! 运行目录是公开元数据：共享映射按进程创建时间与 DLL 路径隔离，不包含登录材料或请求正文。
use std::{
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{
            GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, FILETIME, HANDLE,
            INVALID_HANDLE_VALUE,
        },
        System::{
            Memory::{
                CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
                FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
            },
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
    },
};

const magic: &[u8; 8] = b"OBSHOME1";
const headerBytes: usize = 32;
const maxPathUnits: usize = 32768;
const mappingBytes: usize = headerBytes + maxPathUnits * 2;

// 只管理映射视图；映射对象句柄另由 OwnedHandle 持有，避免混淆 Unmap 与 CloseHandle 生命周期。
struct View(MEMORY_MAPPED_VIEW_ADDRESS);
impl Drop for View {
    // 视图从 MapViewOfFile 得到且只释放一次，内核清理失败不在析构中触发二次 panic。
    fn drop(&mut self) {
        unsafe {
            if !self.0.Value.is_null() {
                let _ = UnmapViewOfFile(self.0);
            }
        }
    }
}

// 目标初始化线程发布后保留返回句柄直至进程退出；名称已存在时不覆盖其他对象。
pub fn publish(module: &Path, home: &Path) -> Result<OwnedHandle, &'static str> {
    let pid = std::process::id();
    let created = currentCreationTime()?;
    let encoded = encode(pid, created, home)?;
    let name = HSTRING::from(mappingName(pid, created, module));
    unsafe {
        SetLastError(ERROR_SUCCESS);
    }
    let mapping = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            mappingBytes as u32,
            &name,
        )
    }
    .map_err(|_| "创建运行目录映射失败")?;
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let mapping = unsafe { OwnedHandle::from_raw_handle(mapping.0) };
    if existed {
        return Err("运行目录映射名称已被占用");
    }
    let view = View(unsafe {
        MapViewOfFile(
            HANDLE(mapping.as_raw_handle()),
            FILE_MAP_WRITE,
            0,
            0,
            mappingBytes,
        )
    });
    if view.0.Value.is_null() {
        return Err("写入运行目录映射失败");
    }
    unsafe {
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), view.0.Value.cast::<u8>(), encoded.len());
    }
    // 就绪事件在本函数返回后发布；此后映射内容不再变化，读方不会观察到半条元数据。
    drop(view);
    Ok(mapping)
}

// 宿主仅在模块就绪后读取；PID、创建时间和头部长度都必须匹配本次候选，不接受陈旧实例。
pub fn read(module: &Path, pid: u32, created: u64) -> Result<PathBuf, &'static str> {
    let name = HSTRING::from(mappingName(pid, created, module));
    let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, &name) }
        .map_err(|_| "读取运行目录映射失败")?;
    let mapping = unsafe { OwnedHandle::from_raw_handle(mapping.0) };
    let view = View(unsafe {
        MapViewOfFile(
            HANDLE(mapping.as_raw_handle()),
            FILE_MAP_READ,
            0,
            0,
            mappingBytes,
        )
    });
    if view.0.Value.is_null() {
        return Err("打开运行目录视图失败");
    }
    let bytes = unsafe { std::slice::from_raw_parts(view.0.Value.cast::<u8>(), mappingBytes) };
    decode(bytes, pid, created)
}

// 目录沿用 Windows UTF-16，原样保留 Unicode 路径；长度、绝对路径和嵌入 NUL 在发布前校验。
fn encode(pid: u32, created: u64, home: &Path) -> Result<Vec<u8>, &'static str> {
    let path: Vec<u16> = home.as_os_str().encode_wide().collect();
    if !home.is_absolute() || path.is_empty() || path.len() >= maxPathUnits || path.contains(&0) {
        return Err("运行目录不是有效绝对路径");
    }
    let mut bytes = vec![0u8; headerBytes];
    bytes[..8].copy_from_slice(magic);
    bytes[8..12].copy_from_slice(&pid.to_le_bytes());
    bytes[16..24].copy_from_slice(&created.to_le_bytes());
    bytes[24..28].copy_from_slice(&(path.len() as u32).to_le_bytes());
    for unit in path {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(bytes)
}

// 先验证范围再解码；协议为定长小端头和有界 UTF-16，不对路径做有损字符串转换。
fn decode(bytes: &[u8], pid: u32, created: u64) -> Result<PathBuf, &'static str> {
    if bytes.len() < headerBytes || &bytes[..8] != magic {
        return Err("运行目录头部无效");
    }
    if bytes[12..16]
        .iter()
        .chain(&bytes[28..32])
        .any(|byte| *byte != 0)
    {
        return Err("运行目录头部含未知标志");
    }
    if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != pid
        || u64::from_le_bytes(bytes[16..24].try_into().unwrap()) != created
    {
        return Err("运行目录进程实例不匹配");
    }
    let length = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    if length == 0 || length >= maxPathUnits || headerBytes + length * 2 > bytes.len() {
        return Err("运行目录长度无效");
    }
    let units: Vec<u16> = bytes[headerBytes..headerBytes + length * 2]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    if units.contains(&0) {
        return Err("运行目录含 NUL");
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
    if !path.is_absolute() {
        return Err("运行目录不是绝对路径");
    }
    Ok(path)
}

// 创建时间也进入对象名，旧读者保留句柄时不会阻止新 PID 实例发布；名称不是鉴权机制。
fn mappingName(pid: u32, created: u64, module: &Path) -> String {
    format!(
        "{}-Home-{created:016x}",
        crate::hook_ready::event_name(pid, module)
    )
}

// 以本进程真实 FILETIME 标识映射实例，不复用线程创建时间或低精度秒时间。
pub fn currentCreationTime() -> Result<u64, &'static str> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|_| "读取运行进程创建时间失败")?;
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

#[cfg(test)]
#[path = "../tests/unit/runtimeHomeTests.rs"]
mod tests;
