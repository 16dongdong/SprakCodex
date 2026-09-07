//! 响应 ID 是网络与客户端事件共有的去重依据；新格式与主机解耦，同时识别历史主机相关散列。
use sha2::{Digest, Sha256};

// 返回新的内部标识与两个既有格式别名，不持久化原响应 ID，也不从客户端事件猜测实际网络主机。
pub(super) fn traces(response: &str) -> (String, Vec<String>) {
    let canonical = format!("observationResponse:{:x}", Sha256::digest(response));
    let aliases = super::observationHosts
        .iter()
        .map(|host| {
            format!(
                "observation:{:x}",
                Sha256::digest(format!("{host}:{response}"))
            )
        })
        .collect();
    (canonical, aliases)
}
