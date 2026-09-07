//! 专用数据库线程与网络运行时同属宿主进程；有界队列施加背压，不静默丢弃统计。
use super::usageParser::UsageParser;
use codexmanager_core::storage::{now_ts, RequestLog, Storage};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use tokio::sync::mpsc;

#[derive(Default)]
pub(super) struct Counters {
    pub written: AtomicU64,
    pub errors: AtomicU64,
}

pub(super) struct Exchange {
    pub host: String,
    pub path: String,
    pub method: String,
    pub protocol: String,
    pub started: Instant,
    pub created: i64,
    pub identity: String,
}

impl Exchange {
    // 在接收请求或 response.created 时建立时间基准；随机标识只用于没有上游响应 ID 的失败请求。
    pub fn new(host: &str, path: &str, protocol: &str) -> Self {
        Self {
            host: host.into(),
            path: path.into(),
            method: "POST".into(),
            protocol: protocol.into(),
            started: Instant::now(),
            created: now_ts(),
            identity: format!("{:032x}", rand::random::<u128>()),
        }
    }
}

pub(super) struct Record {
    request: RequestLog,
    parsed: UsageParser,
}

#[derive(Clone)]
pub(super) struct RecordSink {
    pub sender: mpsc::Sender<Record>,
    pub counters: Arc<Counters>,
}

impl RecordSink {
    // 数据库连接由工作线程独占；启动失败通过 ready 返回，不把运行状态提前标为成功。
    pub fn start(path: std::path::PathBuf) -> Result<(Self, std::thread::JoinHandle<()>), String> {
        let (sender, mut receiver) = mpsc::channel::<Record>(256);
        let counters = Arc::new(Counters::default());
        let workerCounters = counters.clone();
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("observationDatabase".into())
            .spawn(move || {
                let storage = match Storage::open(&path) {
                    Ok(storage) => storage,
                    Err(_) => {
                        let _ = ready.send(Err("打开观测数据库失败".to_string()));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(record) = receiver.blocking_recv() {
                    match storage.insertObservation(
                        &record.request,
                        &record.parsed.usage,
                        record
                            .parsed
                            .model
                            .as_deref()
                            .map(crate::models_v2::policy_catalog_slug),
                    ) {
                        Ok(true) => {
                            workerCounters.written.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(false) => {}
                        Err(_) => {
                            workerCounters.errors.fetch_add(1, Ordering::Relaxed);
                            log::error!("直连观测记录写入失败，事务已回滚");
                        }
                    }
                }
            })
            .map_err(|_| "启动观测数据库线程失败".to_string())?;
        started
            .recv()
            .map_err(|_| "观测数据库线程提前退出".to_string())??;
        Ok((Self { sender, counters }, worker))
    }

    // 完整响应只提交一次；持久化去重键由主机和响应 ID 哈希组成，重连重放也不会重复计数。
    pub async fn finish(&self, exchange: Exchange, parsed: UsageParser, status: u16) {
        let identity = parsed.responseId.as_deref().unwrap_or(&exchange.identity);
        let digest = Sha256::digest(format!("{}:{identity}", exchange.host));
        let request = RequestLog {
            trace_id: Some(format!("observation:{digest:x}")),
            request_path: exchange.path.clone(),
            method: exchange.method,
            request_type: Some(exchange.protocol),
            model: parsed.model.clone(),
            upstream_url: Some(format!("https://{}{}", exchange.host, exchange.path)),
            status_code: Some(i64::from(status)),
            duration_ms: Some(exchange.started.elapsed().as_millis().min(i64::MAX as u128) as i64),
            error: parsed.problem.map(str::to_owned),
            created_at: exchange.created,
            ..Default::default()
        };
        if self.sender.send(Record { request, parsed }).await.is_err() {
            self.counters.errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}
