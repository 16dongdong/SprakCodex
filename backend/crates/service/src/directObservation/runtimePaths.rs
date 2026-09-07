//! 注入路径在一次运行开始时固定，配置与 DLL 同目录；停止复用同一路径而不重新读取环境。
use std::path::{Path, PathBuf};

const moduleFileName: &str = "cphook.dll";
const configFileName: &str = "hook.json";

// 启动时解析显式 DLL 或安装资源；Windows 资源缺失直接失败，非 Windows 仅使用显式代理入口。
pub(super) fn resolve() -> Result<Option<PathBuf>, String> {
    if !cfg!(windows) {
        return Ok(None);
    }
    let executable = std::env::current_exe().map_err(|_| "读取宿主路径失败")?;
    let explicit = std::env::var_os("CODEXMANAGER_OBSERVATION_DLL").map(PathBuf::from);
    resolveModule(&executable, explicit.as_deref()).map(Some)
}

// 显式路径必须为绝对路径且指向文件；只按已约定的 Tauri 安装布局查找，不回退到外部源码目录。
fn resolveModule(executable: &Path, explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(module) = explicit {
        if !module.is_absolute() {
            return Err("观测 DLL 必须使用绝对路径".into());
        }
        return validateModule(module);
    }
    let directory = executable.parent().ok_or("宿主路径缺少父目录")?;
    for folder in ["", "resources", "Resources"] {
        let candidate = directory.join(folder).join(moduleFileName);
        if candidate.is_file() {
            return validateModule(&candidate);
        }
    }
    Err("安装目录缺少观测 DLL，请先构建并安装完整资源".into())
}

// 解析后的路径用于远程 LoadLibraryW；目录和缺失文件不得被当成已安装模块。
fn validateModule(module: &Path) -> Result<PathBuf, String> {
    if !module.is_file() {
        return Err("指定观测 DLL 不是有效文件".into());
    }
    std::fs::canonicalize(module).map_err(|_| "解析观测 DLL 绝对路径失败".into())
}

// DLL 从自身目录读取配置，宿主不得用 current_exe 目录代替；输入来自启动时已验证的模块路径。
pub(super) fn configPath(module: &Path) -> Result<PathBuf, String> {
    Ok(module
        .parent()
        .ok_or("观测 DLL 路径缺少父目录")?
        .join(configFileName))
}

// 在 DLL 所在目录发布配置；端口为零明确关闭 TCP 改连，序列化和文件错误均向宿主返回。
pub(super) fn writeRelayConfig(
    path: &Path,
    port: u16,
    certificate: Option<&Path>,
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
    };
    let encoded = serde_json::to_vec(&configuration).map_err(|_| "生成注入配置失败")?;
    writeAtomically(path, &encoded)
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
