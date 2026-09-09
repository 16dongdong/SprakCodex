//! 常驻发现与有界加载调度：一个目标的 loader 等待不能阻塞其他目标的发现和接入。
use super::processInjector::{self, ProcessCandidate};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const scanInterval: Duration = Duration::from_millis(50);
const retryInterval: Duration = Duration::from_secs(1);
const maxConcurrentLoads: usize = 4;

// 状态绑定完整进程实例；Loading 在目录消失时仍保留，直到对应本地事务回收。
enum Attempt {
    Loading,
    Ready,
    RetryAt(Instant),
}

// 生产加载与目录注册共用一个事务；取消后不启动加载，也不注册已停用运行期的目录。
pub(super) async fn run(
    image: &'static [u8],
    cancel: CancellationToken,
    select: impl Fn() -> Result<Vec<ProcessCandidate>, String> + Send + Sync + 'static,
    onReady: impl Fn(&ProcessCandidate) -> Result<(), String> + Send + Sync + 'static,
) {
    let loadingCancel = cancel.clone();
    runWithLoader(cancel, select, move |candidate| {
        if loadingCancel.is_cancelled() {
            return Err("观测加载已取消".into());
        }
        processInjector::inject(candidate, image)?;
        if loadingCancel.is_cancelled() {
            return Err("观测目录注册已取消".into());
        }
        onReady(candidate)
    })
    .await;
}

// 同步原生操作在阻塞线程执行；调度器持续扫描，以固定上限隔离慢加载，取消时排空而非遗弃事务。
async fn runWithLoader(
    cancel: CancellationToken,
    select: impl Fn() -> Result<Vec<ProcessCandidate>, String> + Send + Sync + 'static,
    load: impl Fn(&ProcessCandidate) -> Result<(), String> + Send + Sync + 'static,
) {
    let select = Arc::new(select);
    let load = Arc::new(load);
    let mut attempts = HashMap::new();
    let mut jobs = FuturesUnordered::new();
    let mut interval = tokio::time::interval(scanInterval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            Some(completed) = jobs.next(), if !jobs.is_empty() => {
                finishAttempt(&mut attempts, completed);
                continue;
            }
            _ = interval.tick() => {}
        }
        let select = select.clone();
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
        let current: HashSet<_> = candidates.iter().cloned().collect();
        attempts.retain(|candidate, attempt| {
            current.contains(candidate) || matches!(attempt, Attempt::Loading)
        });
        for candidate in candidates {
            if cancel.is_cancelled() || jobs.len() == maxConcurrentLoads {
                break;
            }
            match attempts.get(&candidate) {
                Some(Attempt::Loading | Attempt::Ready) => continue,
                Some(Attempt::RetryAt(retry)) if *retry > Instant::now() => continue,
                _ => {}
            }
            attempts.insert(candidate.clone(), Attempt::Loading);
            let load = load.clone();
            let target = candidate.clone();
            let started = Instant::now();
            let worker = tokio::task::spawn_blocking(move || load(&target));
            // 即使阻塞任务 panic，外层仍保留进程身份，完成时解除 Loading 并按限速策略重试。
            jobs.push(async move { (candidate, started, worker.await) });
        }
    }
    // Windows 远程加载有独立超时和延迟清理所有权；此处至少等待已派发的本地工作完成交接。
    while let Some(completed) = jobs.next().await {
        finishAttempt(&mut attempts, completed);
    }
}

// 所有完成路径都退出 Loading；失败只重试同一实例，不阻塞目录扫描或清除其他实例的成功状态。
fn finishAttempt(
    attempts: &mut HashMap<ProcessCandidate, Attempt>,
    completed: (
        ProcessCandidate,
        Instant,
        Result<Result<(), String>, tokio::task::JoinError>,
    ),
) {
    let (candidate, started, result) = completed;
    let pid = candidate.pid;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            log::error!("观测进程加载任务失败 pid={pid}：{error}");
            Err("观测进程加载任务异常退出".into())
        }
    };
    let state = match result {
        Ok(()) => {
            log::info!(
                "观测模块就绪 pid={pid} 加载耗时毫秒={}",
                started.elapsed().as_millis()
            );
            Attempt::Ready
        }
        Err(error) => {
            log::debug!("观测进程接入失败 pid={pid}：{error}");
            Attempt::RetryAt(Instant::now() + retryInterval)
        }
    };
    attempts.insert(candidate, state);
}

#[cfg(test)]
#[path = "../../tests/observation/processMonitorTests.rs"]
mod tests;
