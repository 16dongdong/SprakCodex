//! 观测载荷与配置都驻留内存；磁盘只保留公开证书和业务侧完成事件记录。
use std::path::Path;

#[cfg(windows)]
static embeddedModule: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/observationHook.dll"));

// Windows 返回链接进宿主映像的只读 PE 字节；其他平台没有原生进程载荷。
pub(super) fn moduleImage() -> Option<&'static [u8]> {
    #[cfg(windows)]
    {
        Some(embeddedModule)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

// 发布句柄固定拥有当前运行期的命名页文件映射；Drop 后配置对象随即失效。
#[cfg(windows)]
pub(super) struct RelayPublisher(cpcommon::relayMemory::Publisher);

// 启动时创建唯一控制映射；已有映射表示另一个宿主运行期仍然活跃。
#[cfg(windows)]
pub(super) fn createRelayPublisher() -> Result<Option<RelayPublisher>, String> {
    cpcommon::relayMemory::Publisher::create(cpcommon::relayContract::deploymentIdentity)
        .map(RelayPublisher)
        .map(Some)
        .map_err(str::to_owned)
}

// 非 Windows 通过子进程代理环境接入，不创建 Windows 命名对象。
#[cfg(not(windows))]
pub(super) struct RelayPublisher;

#[cfg(not(windows))]
pub(super) fn createRelayPublisher() -> Result<Option<RelayPublisher>, String> {
    Ok(None)
}

// 在 DLL 所在目录发布配置；端口为零明确关闭 TCP 改连，序列化和文件错误均向宿主返回。
pub(super) fn writeRelayConfig(
    publisher: &RelayPublisher,
    port: u16,
    certificate: Option<&Path>,
    completionDirectory: Option<&Path>,
) -> Result<(), String> {
    // 配置会在另一个进程读取；相对路径会错误依赖目标工作目录，缺失证书也不应发布为可用状态。
    if certificate.is_some_and(|path| !path.is_absolute() || !path.is_file()) {
        return Err("观测公开证书必须为现有文件的绝对路径".into());
    }
    // 非零配置必须由实际 serve 所在线程发布；宿主崩溃或该线程退出后，DLL 持有的线程对象立即失效。
    #[cfg(windows)]
    let owner = if port != 0 {
        Some(cpcommon::runtimeLease::currentIdentity()?)
    } else {
        None
    };
    #[cfg(not(windows))]
    let owner = None;
    let configuration = cpcommon::relayContract::RelayConfig {
        relayPort: port,
        forceProxyTcp: port != 0,
        owner,
        caCertificatePath: certificate.map(Path::to_owned),
        completionEnabled: port != 0 && completionDirectory.is_some(),
        completionDirectory: completionDirectory.map(Path::to_owned),
    };
    writeRelaySnapshot(publisher, &configuration)
}

// 已构造配置统一经同一序列化与映射写入路径发布，测试可替换 owner 验证生命周期失效。
pub(super) fn writeRelaySnapshot(
    publisher: &RelayPublisher,
    configuration: &cpcommon::relayContract::RelayConfig,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(configuration).map_err(|_| "生成注入配置失败")?;
    #[cfg(windows)]
    {
        publisher.0.write(&encoded).map_err(str::to_owned)
    }
    #[cfg(not(windows))]
    {
        let _ = (publisher, encoded);
        Ok(())
    }
}

// 配置与公开证书都可能被其他进程读取；同目录写完再替换，失败只清理本次临时文件。
pub(super) fn writeAtomically(path: &Path, encoded: &[u8]) -> Result<(), String> {
    // 同目录临时文件再原子替换，避免 DLL 在截断写入与写完之间读到残缺 JSON。
    let staging = path.with_extension(format!("{:032x}.pending", rand::random::<u128>()));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|_| "创建观测文件临时副本失败")?;
    let result = (|| {
        use std::io::Write;
        output.write_all(encoded)?;
        output.sync_all()?;
        drop(output);
        std::fs::rename(&staging, path)
    })();
    if result.is_err() {
        match std::fs::remove_file(&staging) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("发布观测文件失败且临时副本清理失败".into()),
        }
    }
    result.map_err(|_| "发布观测文件失败".into())
}

#[cfg(test)]
#[path = "../../tests/observation/runtimePathsTests.rs"]
mod tests;
