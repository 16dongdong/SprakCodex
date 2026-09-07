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
    aliases: Vec<String>,
    pricingAllowed: bool,
    committed: Option<std::sync::mpsc::SyncSender<bool>>,
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
                    let saved = storage.insertObservation(
                        &record.request,
                        &record.parsed.usage,
                        record
                            .parsed
                            .model
                            .as_deref()
                            .filter(|_| record.pricingAllowed)
                            .map(crate::models_v2::policy_catalog_slug),
                        &record.aliases,
                    );
                    let success = saved.is_ok();
                    match saved {
                        Ok(true) => {
                            workerCounters.written.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(false) => {}
                        Err(_) => {
                            workerCounters.errors.fetch_add(1, Ordering::Relaxed);
                            log::error!("直连观测记录写入失败，事务已回滚");
                        }
                    }
                    // 接收者可能超时后重试；确认失败不撤销已提交记录，响应去重阻止重复计数。
                    if let Some(committed) = record.committed {
                        let _ = committed.send(success);
                    }
                }
            })
            .map_err(|_| "启动观测数据库线程失败".to_string())?;
        started
            .recv()
            .map_err(|_| "观测数据库线程提前退出".to_string())??;
        Ok((Self { sender, counters }, worker))
    }

    // 完整响应以响应 ID 生成跨来源标识，同时识别旧主机散列；失败请求仍使用本次随机身份。
    pub async fn finish(&self, exchange: Exchange, parsed: UsageParser, status: u16) {
        let (trace, aliases) = match parsed.responseId.as_deref() {
            Some(response) => super::responseIdentity::traces(response),
            None => (
                format!(
                    "observation:{:x}",
                    Sha256::digest(format!("{}:{}", exchange.host, exchange.identity))
                ),
                Vec::new(),
            ),
        };
        let request = RequestLog {
            trace_id: Some(trace),
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
        if self
            .sender
            .send(Record {
                request,
                parsed,
                aliases,
                pricingAllowed: true,
                committed: None,
            })
            .await
            .is_err()
        {
            self.counters.errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    // 客户端完成事件只报告已知模型和 usage；状态、URL、耗时均保持未知，不伪造成 HTTP 抓包。
    pub(super) fn clientCompleted(
        &self,
        parsed: UsageParser,
        timestamp: i64,
        pricingAllowed: bool,
    ) -> Result<(), String> {
        let response = parsed
            .responseId
            .as_deref()
            .ok_or("客户端事件缺少响应 ID")?;
        let (trace, aliases) = super::responseIdentity::traces(response);
        let request = RequestLog {
            trace_id: Some(trace),
            request_type: Some(codexmanager_core::storage::observationClientRequestType.into()),
            model: parsed.model.clone(),
            created_at: timestamp,
            error: (!pricingAllowed)
                .then(|| "客户端报告了尚未表达的缓存写入计费，用量保留而费用未知".into()),
            ..Default::default()
        };
        let (committed, confirmation) = std::sync::mpsc::sync_channel(1);
        self.sender
            .blocking_send(Record {
                request,
                parsed,
                aliases,
                pricingAllowed,
                committed: Some(committed),
            })
            .map_err(|_| "客户端事件写入队列已关闭".to_owned())?;
        match confirmation.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(true) => Ok(()),
            Ok(false) => Err("客户端事件事务失败，保留读取位置供重试".into()),
            Err(_) => Err("客户端事件提交确认超时，保留读取位置供重试".into()),
        }
    }
}
