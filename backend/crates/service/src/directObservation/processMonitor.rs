//! 常驻进程发现与加载调度；候选来源与加载机制分离，测试可限定自建进程而不接触用户其他会话。
use super::processInjector::{self, ProcessCandidate};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const scanInterval: Duration = Duration::from_millis(50);

// 生产使用完整候选目录，测试提供同一目录的隔离选择；取消后不开始新加载，已开始的事务按自身规则回收。
pub(super) async fn run(
    module: PathBuf,
    cancel: CancellationToken,
    select: impl Fn() -> Result<Vec<ProcessCandidate>, String> + Send + Sync + 'static,
    onReady: impl Fn(&ProcessCandidate, &std::path::Path) -> Result<(), String> + Send + Sync + 'static,
) {
    let select = Arc::new(select);
    let mut injected = HashSet::new();
    let mut interval = tokio::time::interval(scanInterval);
    // 加载等待期间的旧 tick 没有补做价值，跳过而非突发补扫，保持固定的目录读取负载。
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _ = cancel.cancelled() => break, _ = interval.tick() => {} }
        // 取消与首个立即 tick 可能同时就绪；二次检查避免停用后仍开始一次新的目录枚举。
        if cancel.is_cancelled() {
            break;
        }
        let select = select.clone();
        // 系统目录枚举会调用同步原生 API，不占用监听和 SSE 解码的 Tokio 工作线程。
        let candidates = match tokio::task::spawn_blocking(move || select()).await {
            Ok(Ok(candidates)) => candidates,
            Ok(Err(error)) => {
                log::error!("观测进程目录读取失败：{error}");
                continue;
            }
            Err(error) => {
                log::error!("观测进程枚举任务失败：{error}");
                continue;
            }
        };
        let current: HashSet<_> = candidates.into_iter().collect();
        injected.retain(|candidate| current.contains(candidate));
        for candidate in current {
            if cancel.is_cancelled() {
                break;
            }
            if injected.contains(&candidate) {
                continue;
            }
            let pid = candidate.pid;
            let target = candidate.clone();
            let path = module.clone();
            let started = Instant::now();
            match tokio::task::spawn_blocking(move || processInjector::inject(&target, &path)).await
            {
                Ok(Ok(())) => {
                    if cancel.is_cancelled() {
                        break;
                    }
                    if let Err(error) = onReady(&candidate, &module) {
                        log::error!("观测运行目录接入失败 pid={pid}：{error}");
                        continue;
                    }
                    injected.insert(candidate);
                    log::info!(
                        "观测模块就绪 pid={pid} 加载耗时毫秒={}",
                        started.elapsed().as_millis()
                    );
                }
                Ok(Err(error)) => log::debug!("观测进程加载失败 pid={pid}：{error}"),
                Err(error) => log::error!("观测进程加载任务失败 pid={pid}：{error}"),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/observation/processMonitorTests.rs"]
mod tests;
