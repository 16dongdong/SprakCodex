//! Relay 配置通过命名页文件映射发布；载荷只读快照，不在安装目录产生模块或控制文件。
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::copy_nonoverlapping,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Mutex,
    },
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{
            GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE,
            INVALID_HANDLE_VALUE,
        },
        System::Memory::{
            CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
            FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
        },
    },
};

const magic: u64 = u64::from_le_bytes(*b"OBSCFG11");
const headerBytes: usize = 16;
pub const maxConfigBytes: usize = 64 * 1024;
const mappingBytes: usize = headerBytes + maxConfigBytes;

// 映射视图与对象句柄必须分别释放，避免把 UnmapViewOfFile 和 CloseHandle 的所有权混淆。
struct View(MEMORY_MAPPED_VIEW_ADDRESS);

impl Drop for View {
    // 析构只处理当前成功映射的视图；内核清理错误不触发二次故障。
    fn drop(&mut self) {
        if !self.0.Value.is_null() {
            unsafe {
                let _ = UnmapViewOfFile(self.0);
            }
        }
    }
}

// 配置发布句柄由宿主运行期持有；宿主退出后对象自动消失，目标进程不会沿用失效端口。
pub struct Publisher {
    mapping: OwnedHandle,
    sequence: AtomicU32,
    writerLock: Mutex<()>,
}

impl Publisher {
    // 创建独占版本化映射；已有发布者表示另一运行期仍持有同一控制平面。
    pub fn create(identity: &str) -> Result<Self, &'static str> {
        let name = HSTRING::from(mappingName(identity));
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
        .map_err(|_| "创建观测配置映射失败")?;
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let mapping = unsafe { OwnedHandle::from_raw_handle(mapping.0) };
        if existed {
            return Err("观测配置映射已被占用");
        }
        Ok(Self {
            mapping,
            sequence: AtomicU32::new(0),
            writerLock: Mutex::new(()),
        })
    }

    // 单次复制先写正文和长度、最后写魔数；读方只接受前后头一致的完整快照。
    pub fn write(&self, encoded: &[u8]) -> Result<(), &'static str> {
        if encoded.is_empty() || encoded.len() > maxConfigBytes {
            return Err("观测配置长度无效");
        }
        let _writer = self.writerLock.lock().map_err(|_| "观测配置发布锁损坏")?;
        let view = View(unsafe {
            MapViewOfFile(
                HANDLE(self.mapping.as_raw_handle()),
                FILE_MAP_WRITE,
                0,
                0,
                mappingBytes,
            )
        });
        if view.0.Value.is_null() {
            return Err("写入观测配置映射失败");
        }
        let target = view.0.Value.cast::<u8>();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let metadata = u64::from(encoded.len() as u32) | (u64::from(sequence) << 32);
        unsafe {
            // 页映射天然满足 u64 对齐；先撤销魔数，再提交正文和带序列的元数据，最后一次性发布魔数。
            AtomicU64::from_ptr(target.cast()).store(0, Ordering::Release);
            copy_nonoverlapping(encoded.as_ptr(), target.add(headerBytes), encoded.len());
            AtomicU64::from_ptr(target.add(8).cast()).store(metadata, Ordering::Relaxed);
            AtomicU64::from_ptr(target.cast()).store(magic, Ordering::Release);
        }
        Ok(())
    }
}

// 读取方复制前后各验证一次头部；并发发布导致的不一致视为本次无配置，不缓存半条 JSON。
pub fn read(identity: &str) -> Result<Vec<u8>, &'static str> {
    let name = HSTRING::from(mappingName(identity));
    let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, &name) }
        .map_err(|_| "观测配置映射不存在")?;
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
        return Err("读取观测配置映射失败");
    }
    let source = view.0.Value.cast::<u8>();
    let firstMagic = unsafe { AtomicU64::from_ptr(source.cast()).load(Ordering::Acquire) };
    let firstMetadata =
        unsafe { AtomicU64::from_ptr(source.add(8).cast()).load(Ordering::Relaxed) };
    if firstMagic != magic {
        return Err("观测配置头部无效");
    }
    let length = firstMetadata as u32 as usize;
    if length == 0 || length > maxConfigBytes {
        return Err("观测配置长度无效");
    }
    let mut encoded = vec![0u8; length];
    unsafe { copy_nonoverlapping(source.add(headerBytes), encoded.as_mut_ptr(), length) };
    std::sync::atomic::fence(Ordering::Acquire);
    let secondMetadata =
        unsafe { AtomicU64::from_ptr(source.add(8).cast()).load(Ordering::Relaxed) };
    let secondMagic = unsafe { AtomicU64::from_ptr(source.cast()).load(Ordering::Acquire) };
    if firstMagic != secondMagic || firstMetadata != secondMetadata {
        return Err("观测配置发布中");
    }
    Ok(encoded)
}

// 名称只使用固定前缀和 FNV-1a 标识，不将配置内容或本机路径暴露到对象目录。
fn mappingName(identity: &str) -> String {
    let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("Local\\ObservationRelayConfig-{hash:016x}")
}

#[cfg(test)]
#[path = "../tests/unit/relayMemoryTests.rs"]
mod tests;
