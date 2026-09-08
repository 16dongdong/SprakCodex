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
    // 原生文件确认数与唯一请求数分开，便于验证跨来源去重而不把同一请求计成两次。
    pub nativeAccepted: AtomicU64,
}

pub(super) struct Exchange {
    pub host: String,
    pub path: String,
    pub method: String,
    pub protocol: String,
    pub started: Instant,
    pub created: i64,
    pub identity: String,
    pub requestBody: Arc<std::sync::Mutex<super::detailCapture::Capture>>,
    pub requestHeaders: serde_json::Value,
    pub responseHeaders: serde_json::Value,
    pub firstResponseMs: Option<i64>,
    pub account: Option<String>,
    pub accountLabel: Option<String>,
    pub keyFingerprint: Option<String>,
    pub routingMode: String,
    pub routingSessionId: Option<String>,
    pub routingSource: Option<String>,
    pub routingReason: String,
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
            requestBody: Default::default(),
            requestHeaders: serde_json::Value::Null,
            responseHeaders: serde_json::Value::Null,
            firstResponseMs: None,
            account: None,
            accountLabel: None,
            keyFingerprint: None,
            routingMode: "passthrough".to_string(),
            routingSessionId: None,
            routingSource: None,
            routingReason: "routing_not_evaluated".to_string(),
            identity: format!("{:032x}", rand::random::<u128>()),
        }
    }

    /// 保存本次请求最终采用的分流决策；只记录会话标识和原因，不复制访问令牌。
    pub fn captureRouting(&mut self, decision: &crate::sessionRouting::RouteDecision) {
        let (mode, sessionId, source, reason) = crate::sessionRouting::decisionMetadata(decision);
        self.routingMode = mode.to_string();
        self.routingSessionId = sessionId.map(str::to_string);
        self.routingSource = source.map(str::to_string);
        self.routingReason = reason.to_string();
        if let Some(label) = crate::sessionRouting::routedAccountLabel(decision) {
            self.accountLabel = Some(label.to_string());
        }
    }
    // 认证仅参与不可逆指纹，账号标识来自显式请求头；不借用 Manager 账号池身份。
    pub fn captureHeaders(&mut self, headers: &hyper::HeaderMap) {
        self.requestHeaders = super::detailCapture::headers(headers);
        // JWT 只解码显示字段，不用于授权判断；访问凭据不写入日志或身份快照。
        self.accountLabel = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .and_then(|(_, token)| codexmanager_core::auth::parse_id_token_claims(token).ok())
            .and_then(|claims| {
                let email = claims.resolved_email();
                let name = claims
                    .profile
                    .as_ref()
                    .and_then(|profile| profile.name.as_deref())
                    .map(str::trim)
                    .filter(|name| !name.is_empty());
                match (email, name) {
                    (Some(email), Some(name)) if email != name => Some(format!("{name} <{email}>")),
                    (Some(email), _) => Some(email.to_owned()),
                    (_, Some(name)) => Some(name.to_owned()),
                    _ => None,
                }
            })
            .filter(|label| label.len() <= 512 && !label.chars().any(char::is_control));
        self.account = headers
            .get("chatgpt-account-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.len() <= 256)
            .map(str::to_owned);
        self.keyFingerprint = headers
            .get("authorization")
            .map(|value| format!("direct:{:x}", Sha256::digest(value.as_bytes())));
    }
}

pub(super) struct Record {
    request: RequestLog,
    details: Option<String>,
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
                    let saved = storage.insertObservationDetails(
                        &record.request,
                        &record.parsed.usage,
                        codexmanager_core::storage::ObservationContext {
                            pricingModel: record
                                .parsed
                                .model
                                .as_deref()
                                .filter(|_| record.pricingAllowed)
                                .map(crate::models_v2::policy_catalog_slug),
                            legacyTraces: &record.aliases,
                            details: record.details.as_deref(),
                        },
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
        // 报文落库和脱敏耗时不计入客户端请求的网络耗时。
        let duration = exchange.started.elapsed().as_millis().min(i64::MAX as u128) as i64;
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
        let requestBody = exchange
            .requestBody
            .lock()
            .map(|body| {
                body.snapshotEncoded(
                    exchange
                        .requestHeaders
                        .get("content-encoding")
                        .and_then(serde_json::Value::as_str),
                )
            })
            .unwrap_or_else(|_| serde_json::json!({"error":"请求详情锁损坏"}));
        let requestModel = requestBody
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let requestReasoning = requestBody
            .pointer("/reasoning/effort")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let requestTier = requestBody
            .get("service_tier")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let details = serde_json::json!({
            "formatVersion": 3,
            "routing": {
                "mode": exchange.routingMode,
                "sessionId": exchange.routingSessionId,
                "source": exchange.routingSource,
                "reason": exchange.routingReason,
            },
            "request": {"headers":exchange.requestHeaders,"body":requestBody},
            "response": {"headers":exchange.responseHeaders,"body":parsed.body.snapshot()}
        })
        .to_string();
        let request = RequestLog {
            trace_id: Some(trace),
            request_path: exchange.path.clone(),
            method: exchange.method,
            request_type: Some(exchange.protocol),
            model: parsed.model.clone().or(requestModel.clone()),
            client_model: requestModel,
            model_source: Some(
                if parsed.model.is_some() {
                    "upstream"
                } else {
                    "client"
                }
                .into(),
            ),
            account_id: exchange.account,
            account_label: exchange.accountLabel,
            key_id: exchange.keyFingerprint,
            upstream_url: Some(format!("https://{}{}", exchange.host, exchange.path)),
            status_code: Some(i64::from(status)),
            duration_ms: Some(duration),
            first_response_ms: exchange.firstResponseMs,
            route_strategy: Some(exchange.routingMode),
            route_source: exchange.routingSource,
            actual_source_kind: exchange
                .routingSessionId
                .as_ref()
                .map(|_| "session".to_string()),
            actual_source_id: exchange.routingSessionId,
            reasoning_effort: parsed.reasoning.clone().or(requestReasoning),
            service_tier: requestTier.or(parsed.tier.clone()),
            effective_service_tier: parsed.tier.clone(),
            error: parsed
                .diagnostic
                .clone()
                .or_else(|| parsed.problem.map(str::to_owned)),
            created_at: exchange.created,
            ..Default::default()
        };
        if self
            .sender
            .send(Record {
                request,
                details: Some(details),
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
                details: None,
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

#[cfg(test)]
#[path = "../../tests/observation/identityLabelTests.rs"]
mod identityLabelTests;
