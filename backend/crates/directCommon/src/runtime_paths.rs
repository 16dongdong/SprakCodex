//! 运行时文件定位(Host 与代理引擎共享:都按 exe 同目录解析 hook.json / cphook.dll / data)。

use std::path::PathBuf;

/// 当前 exe 所在目录。
/// 返回当前宿主可执行文件所在目录；失败时将底层 IO 错误传回调用方。
pub fn exe_dir() -> std::io::Result<PathBuf> {
    let mut p = std::env::current_exe()?;
    p.pop();
    Ok(p)
}

/// 与 exe 同目录的目标进程注入 DLL。
/// 返回与宿主可执行文件同目录的注入 DLL 路径，不检查文件是否存在。
pub fn cphook_dll() -> std::io::Result<PathBuf> {
    Ok(exe_dir()?.join("cphook.dll"))
}
