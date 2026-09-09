//! Windows 进程发现与内存载荷部署基础层。
//! 部署前必须由观测协议层提供已嵌入的 PE 字节；本模块不修改目标进程环境变量。

use cpcommon::deploymentLifecycle::DeploymentRecord;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
};

const targetProcessNames: &[&str] = &["codex.exe", "codex-app.exe"];
type DeploymentHandler = Arc<dyn Fn(bool, DeploymentRecord) -> Result<(), String> + Send + Sync>;
static deployments: OnceLock<RwLock<BTreeMap<(u32, u64), DeploymentRecord>>> = OnceLock::new();
static deploymentHandler: OnceLock<RwLock<Option<DeploymentHandler>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ProcessCandidate {
    pub pid: u32,
    pub createdAt: u64,
    pub executable: PathBuf,
}

// 过滤 Codex 主进程和 app-server，返回本次扫描的实时 PID；调用方不得缓存 PID 跨重启使用。
#[cfg(windows)]
pub(super) fn findCandidates() -> Result<Vec<ProcessCandidate>, String> {
    Ok(super::processCatalog::targetPids()?
        .into_iter()
        .filter_map(|pid| {
            let candidate = super::nativeInjection::candidate(pid).ok()?;
            isTargetExecutable(&candidate.executable).then_some(candidate)
        })
        .collect())
}

// 非 Windows 保留平台目录实现；目前该平台没有原生加载器，不增加 Windows 专用查询依赖。
#[cfg(not(windows))]
pub(super) fn findCandidates() -> Result<Vec<ProcessCandidate>, String> {
    // 扫描只需要可执行路径，不采集全机环境变量、CPU、磁盘和内存统计。
    let processes = sysinfo::ProcessRefreshKind::new().with_exe(sysinfo::UpdateKind::OnlyIfNotSet);
    let system =
        sysinfo::System::new_with_specifics(sysinfo::RefreshKind::new().with_processes(processes));
    Ok(system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let executable = process.exe()?.to_path_buf();
            if !isTargetExecutable(&executable) {
                return None;
            }
            Some(ProcessCandidate {
                pid: pid.as_u32(),
                createdAt: process.start_time(),
                executable,
            })
        })
        .collect())
}

// 系统目录中的名称不必分配 String；只接受完整 ASCII 目标文件名，Unicode 近似拼写不参与选择。
#[cfg(windows)]
pub(super) fn isTargetWideName(name: &[u16]) -> bool {
    targetProcessNames.iter().any(|target| {
        target.len() == name.len()
            && target.bytes().zip(name).all(|(expected, &actual)| {
                actual < 128 && (actual as u8).eq_ignore_ascii_case(&expected)
            })
    })
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
pub(super) fn inject(candidate: &ProcessCandidate, image: &[u8]) -> Result<(), String> {
    super::nativeInjection::inject(candidate, image).map(|_| ())
}

// 桌面壳在扫描前注册处理器，把后续部署变化同步给独立看门狗；替换处理器不会重复旧事件。
pub(crate) fn setDeploymentHandler(
    handler: impl Fn(bool, DeploymentRecord) -> Result<(), String> + Send + Sync + 'static,
) -> Result<(), String> {
    let mut current = deploymentHandler
        .get_or_init(|| RwLock::new(None))
        .write()
        .map_err(|_| "观测部署处理器锁损坏")?;
    *current = Some(Arc::new(handler));
    Ok(())
}

// 远程初始化成功后按完整进程实例登记；同一记录重复扫描只覆盖，不增加生命周期槽。
pub(super) fn recordDeployment(record: DeploymentRecord) -> Result<(), String> {
    let key = (record.processId, record.createdAt);
    notifyDeployment(true, record.clone())?;
    deployments
        .get_or_init(|| RwLock::new(BTreeMap::new()))
        .write()
        .map_err(|_| "观测部署表锁损坏")?
        .insert(key, record);
    Ok(())
}

// 通知在部署表锁外执行，避免看门狗管道写入反向阻塞加载和卸载事务。
fn notifyDeployment(tracked: bool, record: DeploymentRecord) -> Result<(), String> {
    let handler = deploymentHandler
        .get_or_init(|| RwLock::new(None))
        .read()
        .map_err(|_| "观测部署处理器锁损坏")?
        .clone();
    if let Some(handler) = handler {
        return handler(tracked, record);
    }
    Ok(())
}

// 服务正常停止时主动卸载；失败记录留给仍持有父进程生命周期的看门狗继续处理。
pub(super) fn unloadDeployments() -> Result<(), String> {
    let records = deployments
        .get_or_init(|| RwLock::new(BTreeMap::new()))
        .read()
        .map_err(|_| "观测部署表锁损坏")?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for record in records {
        match cpcommon::deploymentLifecycle::unload(&record) {
            Ok(_) => {
                deployments
                    .get_or_init(|| RwLock::new(BTreeMap::new()))
                    .write()
                    .map_err(|_| "观测部署表锁损坏")?
                    .remove(&(record.processId, record.createdAt));
                if let Err(error) = notifyDeployment(false, record) {
                    log::error!("同步观测卸载记录失败：{error}");
                }
            }
            Err(error) => failures.push(format!("pid={}：{error}", record.processId)),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("卸载观测映像失败：{}", failures.join("；")))
    }
}

// 模块就绪后只读运行目录映射；创建时间来自最新候选，旧 PID 实例的目录不可复用。
#[cfg(windows)]
pub(super) fn runtimeHome(candidate: &ProcessCandidate) -> Result<PathBuf, String> {
    cpcommon::runtimeHome::read(
        cpcommon::relayContract::deploymentIdentity,
        candidate.pid,
        candidate.createdAt,
    )
    .map_err(str::to_owned)
}

// 非 Windows 不查询 Windows 映射；当前原生加载会先明确报告该平台不支持。
#[cfg(not(windows))]
pub(super) fn runtimeHome(_candidate: &ProcessCandidate) -> Result<PathBuf, String> {
    Err("当前平台没有运行目录映射".into())
}

// 非 Windows 宿主只有显式代理入口，不将缺失的原生能力伪装为成功。
#[cfg(not(windows))]
pub(super) fn inject(_candidate: &ProcessCandidate, _image: &[u8]) -> Result<(), String> {
    Err("当前平台没有 Windows 进程注入实现".into())
}
#[cfg(test)]
#[path = "../../tests/observation/processSelectionTests.rs"]
mod tests;
