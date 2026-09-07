//! 模块就绪事件同时绑定 PID 与 DLL 路径，避免另一目录的模块或旧协议事件被误认为本次初始化成功。
use std::path::Path;

/// 宿主与 DLL 按实际加载路径生成相同的版本化名称；事件名用于区分模块实例，不代替鉴权。
pub fn event_name(pid: u32, module: &Path) -> String {
    // FNV-1a 只用于压缩路径标识，不用于密码学；先消除 Windows 扩展路径和大小写差异。
    let normalized = module
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .to_lowercase();
    let hash = normalized
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    // 第六版还要求公开运行目录元数据已发布，避免宿主把未具备目录发现能力的旧模块当成完整就绪。
    format!("Local\\ObservationHookReady6-{pid}-{hash:016x}")
}

#[cfg(test)]
#[path = "../tests/unit/readyEventTests.rs"]
mod tests;
