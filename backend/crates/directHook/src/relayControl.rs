//! 每条新连接读取完整配置；仅复用解析结果和内核句柄，不用文件时间推断运行实例仍然有效。
use cpcommon::{relayContract::RelayConfig, runtimeLease::RuntimeLease};
use std::{io::Read, path::Path, sync::Arc};

const maxConfigBytes: u64 = 64 * 1024;

// 字节内容相同时复用已验证身份；配置不可读时释放缓存，避免残缺写入或删除仍沿用旧端口。
#[derive(Default)]
pub(super) struct RelayControl {
    current: Option<ValidatedConfig>,
}

// 配置与其线程对象绑定为同一快照，端口与公开证书位置不得跨配置版本混用。
struct ValidatedConfig {
    encoded: Vec<u8>,
    settings: Arc<RelayConfig>,
    owner: RuntimeLease,
}

impl RelayControl {
    // 仅在 connect/ConnectEx 决策时调用，不进入逐包发送热路径；错误返回原连接行为而不是旧配置。
    pub(super) fn read(&mut self, path: &Path) -> Option<Arc<RelayConfig>> {
        match self.readCurrent(path) {
            Ok(settings) => Some(settings),
            Err(()) => {
                self.current = None;
                None
            }
        }
    }

    // 限制实际读取字节数，并在发布快照前后核对活跃线程；文件、格式和实例错误均终止本次改连。
    fn readCurrent(&mut self, path: &Path) -> Result<Arc<RelayConfig>, ()> {
        let mut encoded = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| ())?
            .take(maxConfigBytes + 1)
            .read_to_end(&mut encoded)
            .map_err(|_| ())?;
        if encoded.len() as u64 > maxConfigBytes {
            return Err(());
        }
        if let Some(current) = &self.current {
            if current.encoded == encoded {
                return current
                    .owner
                    .isActive()
                    .then(|| current.settings.clone())
                    .ok_or(());
            }
        }
        let bytes = encoded
            .strip_prefix(&[0xEF, 0xBB, 0xBF])
            .unwrap_or(&encoded);
        let settings: RelayConfig = serde_json::from_slice(bytes).map_err(|_| ())?;
        if !settings.forceProxyTcp || settings.relayPort == 0 {
            return Err(());
        }
        if settings
            .caCertificatePath
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(());
        }
        let owner = RuntimeLease::open(settings.owner.ok_or(())?).map_err(|_| ())?;
        let settings = Arc::new(settings);
        self.current = Some(ValidatedConfig {
            encoded,
            settings: settings.clone(),
            owner,
        });
        Ok(settings)
    }
}

#[cfg(test)]
#[path = "../tests/unit/relayControlTests.rs"]
mod tests;
