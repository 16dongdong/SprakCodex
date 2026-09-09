//! 模块事件同时绑定 PID 与部署标识，内存映像不依赖不存在的磁盘路径。

/// 宿主与内存模块按稳定部署标识生成相同名称；哈希只压缩对象名，不承担鉴权。
pub fn event_name(pid: u32, identity: &str) -> String {
    let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("Local\\ObservationHookReady12-{pid}-{hash:016x}")
}

/// 加载事件早于运行期就绪事件，用于阻止初始化失败的内存映像被重复映射。
pub fn loaded_event_name(pid: u32, identity: &str) -> String {
    event_name(pid, identity).replace("Ready12", "Loaded12")
}

#[cfg(test)]
#[path = "../tests/unit/readyEventTests.rs"]
mod tests;
