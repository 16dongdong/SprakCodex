//! 原生完成事件的持久消费：数据库确认前保留文件，宿主或客户端重启不依赖内存游标。
use super::{recordSink::RecordSink, usageParser::UsageParser};
use codexmanager_core::storage::RequestTokenStat;
use cpcommon::completionSpool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub(super) struct Settings {
    pub directory: PathBuf,
    pub allow: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
}

// 线程只归当前宿主所有；退出留下尚未确认的文件，不以删除队列来宣称清理成功。
pub(super) struct Monitor {
    cancel: CancellationToken,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Monitor {
    // 先创建受管目录再启动消费者；来源过滤在读取文件内容前执行，便于测试限定其真实 CLI。
    pub fn start(
        settings: Settings,
        sink: RecordSink,
        cancel: CancellationToken,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&settings.directory).map_err(|_| "创建完成事件目录失败")?;
        let workerCancel = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("completionObservation".into())
            .spawn(move || run(settings, sink, workerCancel))
            .map_err(|_| "启动完成事件消费者失败")?;
        Ok(Self {
            cancel,
            worker: Some(worker),
        })
    }
}
// 扫描与单文件重试分别限速；目录故障只在状态变化时报告，避免停盘期间反复增加相同错误。
fn run(settings: Settings, sink: RecordSink, cancel: CancellationToken) {
    let mut failures = HashMap::new();
    let mut directoryFailed = false;
    while !cancel.is_cancelled() {
        let failed = scan(&settings, &sink, &cancel, &mut failures).is_err();
        if failed && !directoryFailed {
            sink.counters.errors.fetch_add(1, Ordering::Relaxed);
            log::error!("读取完成事件目录失败");
        }
        directoryFailed = failed;
        std::thread::sleep(if failed {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(100)
        });
    }
}

// 每个条目先执行范围检查再读内容，取消后不开始下一个文件；文件失败不阻止其他响应提交。
fn scan(
    settings: &Settings,
    sink: &RecordSink,
    cancel: &CancellationToken,
    failures: &mut HashMap<PathBuf, (Instant, &'static str)>,
) -> Result<(), ()> {
    let entries = std::fs::read_dir(&settings.directory).map_err(|_| ())?;
    failures.retain(|path, _| path.exists());
    for entry in entries {
        if cancel.is_cancelled() {
            break;
        }
        let path = entry.map_err(|_| ())?.path();
        if completionSpool::isReady(&path) && (settings.allow)(&path) {
            processFile(path, sink, failures);
        }
    }
    Ok(())
}

// 同一失败保留文件并在一秒后重试；成功确认后的删除失败也走响应去重路径，不重复计费。
fn processFile(
    path: PathBuf,
    sink: &RecordSink,
    failures: &mut HashMap<PathBuf, (Instant, &'static str)>,
) {
    if failures
        .get(&path)
        .is_some_and(|(time, _)| time.elapsed() < Duration::from_secs(1))
    {
        return;
    }
    match consume(&path, sink) {
        Ok(()) => {
            failures.remove(&path);
        }
        Err(error) => {
            if failures
                .get(&path)
                .is_none_or(|(_, previous)| *previous != error)
            {
                sink.counters.errors.fetch_add(1, Ordering::Relaxed);
                log::error!("消费原生完成事件失败：{error}");
            }
            failures.insert(path, (Instant::now(), error));
        }
    }
}

impl Drop for Monitor {
    // 等待正在进行的数据库确认结束；未确认文件留待下次启动，取消不会强杀客户端或抛弃其事件。
    fn drop(&mut self) {
        self.cancel.cancel();
        if self.worker.take().unwrap().join().is_err() {
            log::error!("完成事件消费者异常退出");
        }
    }
}

// 内容与文件名已由共享协议验证；客户端上下文保持明确来源，原始 HTTP 字段不虚构。
fn consume(path: &Path, sink: &RecordSink) -> Result<(), &'static str> {
    commitFile(path, |completion| {
        let parsed = UsageParser::reported(
            completion.model,
            completion.responseId,
            RequestTokenStat {
                input_tokens: Some(completion.inputTokens),
                cached_input_tokens: Some(completion.cachedInputTokens),
                output_tokens: Some(completion.outputTokens),
                total_tokens: Some(completion.totalTokens),
                reasoning_output_tokens: Some(completion.reasoningOutputTokens),
                ..Default::default()
            },
        );
        sink.clientCompleted(
            parsed,
            completion.timestampMillis / 1000,
            completion.cacheWriteInputTokens == 0,
        )
        .map_err(|_| "完成事件数据库确认失败")
    })?;
    sink.counters.nativeAccepted.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

// 文件事务与数据库提交分离；只有提交回调确认成功才删除，错误原样返回供重启后重放。
fn commitFile(
    path: &Path,
    report: impl FnOnce(completionSpool::Completion) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    report(completionSpool::read(path)?)?;
    // 若删除前宿主退出，响应 ID 去重让下次重放只补确认，不重复增加请求数或费用快照。
    std::fs::remove_file(path).map_err(|_| "完成事件已提交但文件清理失败")
}

#[cfg(test)]
#[path = "../../tests/observation/completionMonitorTests.rs"]
mod tests;
