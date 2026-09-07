//! Windows 进程发现与 DLL 注入基础层。
//! 注入前必须由观测协议层提供对应 DLL 和配置；本模块不修改目标进程环境变量。

use std::path::{Path, PathBuf};

const targetProcessNames: &[&str] = &["codex.exe", "codex-app.exe"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ProcessCandidate {
    pub pid: u32,
    pub createdAt: u64,
    pub executable: PathBuf,
}

// 过滤 Codex 主进程和 app-server，返回本次扫描的实时 PID；调用方不得缓存 PID 跨重启使用。
pub(super) fn findCandidates() -> Vec<ProcessCandidate> {
    // 扫描只需要可执行路径，不采集全机环境变量、CPU、磁盘和内存统计。
    let processes = sysinfo::ProcessRefreshKind::new().with_exe(sysinfo::UpdateKind::OnlyIfNotSet);
    let system =
        sysinfo::System::new_with_specifics(sysinfo::RefreshKind::new().with_processes(processes));
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let executable = process.exe()?.to_path_buf();
            if !isTargetExecutable(&executable) {
                return None;
            }
            #[cfg(windows)]
            {
                let candidate = super::nativeInjection::candidate(pid.as_u32()).ok()?;
                isTargetExecutable(&candidate.executable).then_some(candidate)
            }
            #[cfg(not(windows))]
            {
                Some(ProcessCandidate {
                    pid: pid.as_u32(),
                    createdAt: process.start_time(),
                    executable,
                })
            }
        })
        .collect()
}

// app-server 是通用参数而不是进程身份；只接受指定可执行文件名，避免误接管其他本地服务。
fn isTargetExecutable(executable: &Path) -> bool {
    executable.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        targetProcessNames
            .iter()
            .any(|target| name.eq_ignore_ascii_case(target))
    })
}

// 进程发现与加载事务分离：仅实际 DLL 就绪返回成功，失败语义由 Windows 生命周期模块统一负责。
#[cfg(windows)]
pub(super) fn inject(candidate: &ProcessCandidate, dll: &Path) -> Result<(), String> {
    super::nativeInjection::inject(candidate, dll)
}

// 非 Windows 宿主只有显式代理入口，不将缺失的原生能力伪装为成功。
#[cfg(not(windows))]
pub(super) fn inject(_candidate: &ProcessCandidate, _dll: &Path) -> Result<(), String> {
    Err("当前平台没有 Windows 进程注入实现".into())
}
#[cfg(test)]
#[path = "../../tests/observation/processSelectionTests.rs"]
mod tests;
