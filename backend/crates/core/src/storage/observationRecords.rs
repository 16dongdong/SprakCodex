//! 直连观测只写请求与用量，按模型价格创建费用快照、不修改钱包或平台密钥额度。
use super::{ChargeSnapshotInputV2, RequestLog, RequestTokenStat, Storage};
use rusqlite::{params, OptionalExtension, Result, Transaction};

// 非生成预热是独立的网络请求，原始用量保留供核对，但不混入生成用量或按生成价格估算。
#[allow(non_upper_case_globals)]
pub const observationPrewarmRequestType: &str = "websocketPrewarm";

// 客户端完成事件提供精确 usage，但没有网络路径、HTTP 状态或传输耗时，必须保留独立来源。
#[allow(non_upper_case_globals)]
pub const observationClientRequestType: &str = "clientResponse";

// 观测写入附属参数共用事务，详情为已脱敏序列化 JSON；缺省用于无正文的完成事件。
#[allow(non_snake_case)]
pub struct ObservationContext<'a> {
    pub pricingModel: Option<&'a str>,
    pub legacyTraces: &'a [String],
    pub details: Option<&'a str>,
}

// 合并前的观测数据用于保留请求主键和既有价格；不包含钱包身份或认证信息。
struct Previous {
    id: i64,
    kind: Option<String>,
    model: Option<String>,
    usage: RequestTokenStat,
    pricingModel: Option<String>,
    cost: Option<i64>,
    route: RequestLog,
    modelSource: String,
}

#[allow(non_snake_case)]
impl Storage {
    // 在数据库工作线程内原子写入观测结果。trace_id 必须由观测器生成；
    // 相同响应重放或元数据升级返回 false（不增加请求数）；旧别名用于迁移去重，任一写入失败回滚全部变更。
    pub fn insertObservation(
        &self,
        request: &RequestLog,
        usage: &RequestTokenStat,
        pricingModel: Option<&str>,
        legacyTraces: &[String],
    ) -> Result<bool> {
        self.insertObservationDetails(
            request,
            usage,
            ObservationContext {
                pricingModel,
                legacyTraces,
                details: None,
            },
        )
    }

    // 请求、用量、费用快照和详情在同一事务确认；任一写入失败全量回滚，重放不会留下半条记录。
    pub fn insertObservationDetails(
        &self,
        request: &RequestLog,
        usage: &RequestTokenStat,
        context: ObservationContext<'_>,
    ) -> Result<bool> {
        let ObservationContext {
            pricingModel,
            legacyTraces,
            details,
        } = context;
        let trace = request
            .trace_id
            .as_deref()
            .filter(|trace| !trace.is_empty())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("观测记录缺少去重标识".into()))?;
        if legacyTraces.len() > 2 {
            return Err(rusqlite::Error::InvalidParameterName(
                "观测旧标识数量超过上限".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let previous = loadPrevious(&transaction, trace, legacyTraces)?;
        let client = request.request_type.as_deref() == Some(observationClientRequestType);
        let upgrade = previous.as_ref().is_some_and(|old| {
            (old.kind.as_deref() == Some(observationClientRequestType) && !client)
                || missingFields(old, request, usage)
        });
        if let Some(old) = previous.as_ref().filter(|_| upgrade) {
            let charged: i64 = transaction.query_row("SELECT COUNT(*) FROM app_wallet_ledger_entries WHERE request_log_id=?1 AND entry_kind='request_charge'", [old.id], |row| row.get(0))?;
            if charged != 0 {
                return Err(rusqlite::Error::InvalidParameterName(
                    "已有扣费账目的记录不可由观测器改写".into(),
                ));
            }
        }
        if let Some(old) = previous.as_ref().filter(|_| !upgrade) {
            transaction.execute(
                "UPDATE request_logs SET trace_id=?2 WHERE id=?1",
                params![old.id, trace],
            )?;
            saveDetails(&transaction, old.id, details)?;
            transaction.commit()?;
            return Ok(false);
        }
        let mut request = request.clone();
        let mut usage = usage.clone();
        let incomingModel = request.model.clone();
        let keepNetwork = client
            && previous
                .as_ref()
                .is_some_and(|old| old.kind.as_deref() != Some(observationClientRequestType));
        let sourceModel = if keepNetwork {
            let old = previous.as_ref().unwrap();
            request = old.route.clone();
            request.trace_id = Some(trace.into());
            request.model = request.model.or_else(|| incomingModel.clone());
            if old.model.is_some() {
                old.modelSource.as_str()
            } else {
                "client_context"
            }
        } else if client
            || (request.model.is_none() && previous.as_ref().is_some_and(|old| old.model.is_some()))
        {
            "client_context"
        } else {
            request.model_source.as_deref().unwrap_or("upstream")
        };
        if let Some(old) = &previous {
            ensureConsistentUsage(&old.usage, &usage)?;
            request.model = request.model.or_else(|| old.model.clone());
            usage.input_tokens = usage.input_tokens.or(old.usage.input_tokens);
            usage.cached_input_tokens = usage.cached_input_tokens.or(old.usage.cached_input_tokens);
            usage.output_tokens = usage.output_tokens.or(old.usage.output_tokens);
            usage.total_tokens = usage.total_tokens.or(old.usage.total_tokens);
            usage.reasoning_output_tokens = usage
                .reasoning_output_tokens
                .or(old.usage.reasoning_output_tokens);
        }
        let changedUsage = previous.as_ref().is_none_or(|old| {
            old.model != request.model
                || old.usage.input_tokens != usage.input_tokens
                || old.usage.cached_input_tokens != usage.cached_input_tokens
                || old.usage.output_tokens != usage.output_tokens
        });
        let requestId = writeRequest(
            &transaction,
            &request,
            previous.as_ref().map(|old| old.id),
            sourceModel,
        )?;
        if previous.is_some() {
            transaction.execute(
                "DELETE FROM request_token_stats WHERE request_log_id=?1",
                [requestId],
            )?;
            if changedUsage {
                transaction.execute(
                    "DELETE FROM request_charge_snapshots WHERE request_log_id=?1",
                    [requestId],
                )?;
            }
        }
        let prewarm = request.request_type.as_deref() == Some(observationPrewarmRequestType);
        transaction.execute(
            "INSERT INTO request_token_stats (request_log_id, model, actual_source_kind,
             input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
             usage_included, created_at) VALUES (?1, ?2, ?10, ?3, ?4, ?5, ?6, ?7, ?9, ?8)",
            params![requestId, request.model, usage.input_tokens,
                usage.cached_input_tokens, usage.output_tokens, usage.total_tokens,
                usage.reasoning_output_tokens, request.created_at, !prewarm,
                if client { "clientObservation" } else { "directObservation" }],
        )?;
        let pricingModel = if keepNetwork
            && previous
                .as_ref()
                .is_some_and(|old| old.model.is_some() && old.model != incomingModel)
        {
            previous
                .as_ref()
                .and_then(|old| old.pricingModel.as_deref().or(old.model.as_deref()))
        } else {
            pricingModel
        }
        .or_else(|| {
            previous
                .as_ref()
                .and_then(|old| old.pricingModel.as_deref())
        });
        let pricing = pricingModel
            .filter(|_| !prewarm)
            .zip(usage.input_tokens)
            .zip(usage.output_tokens)
            .zip(usage.cached_input_tokens);
        let retainedCost = previous
            .as_ref()
            .filter(|_| !changedUsage)
            .and_then(|old| old.cost);
        let priced = match (retainedCost, pricing) {
            (Some(cost), _) => {
                transaction.execute("UPDATE request_token_stats SET estimated_cost_usd=CAST(?2 AS REAL)/1000000.0 WHERE request_log_id=?1", params![requestId, cost])?;
                true
            }
            (None, Some((((model, input), output), cached)))
                if self.select_model_price_tier_v2(model, input)?.is_some() =>
            {
                self.recordChargeSnapshotInTransaction(
                    &transaction,
                    &ChargeSnapshotInputV2 {
                        request_log_id: requestId,
                        model_slug: request.model.clone().unwrap_or_default(),
                        pricing_model_slug: Some(model.into()),
                        usage_source: "actual".into(),
                        input_tokens: input,
                        cached_input_tokens: cached,
                        output_tokens: output,
                        rate_multiplier_millis: 1000,
                        ..Default::default()
                    },
                )?;
                true
            }
            _ => false,
        };
        if !priced && !prewarm {
            transaction.execute("UPDATE request_logs SET error=CASE WHEN error IS NULL THEN ?2 ELSE error || ?3 || ?2 END WHERE id=?1",
                params![requestId, "费用未知：缺少完整用量或模型价格，未生成零价快照", "；"])?;
        } else if priced {
            // 补齐用量后原有“费用未知”说明失效；保留其他观测或上游诊断，不抹掉真实错误。
            let diagnostic = request
                .error
                .as_deref()
                .map(|error| {
                    error
                        .split('；')
                        .filter(|part| *part != "费用未知：缺少完整用量或模型价格，未生成零价快照")
                        .collect::<Vec<_>>()
                        .join("；")
                })
                .filter(|error| !error.is_empty());
            transaction.execute(
                "UPDATE request_logs SET error=?2 WHERE id=?1",
                params![requestId, diagnostic],
            )?;
        }
        saveDetails(&transaction, requestId, details)?;
        transaction.commit()?;
        Ok(previous.is_none())
    }
}

// 网络已存在也允许客户端填补缺失计数；完整网络数据仍优先，重复报告不触发整条记录重写。
fn missingFields(previous: &Previous, request: &RequestLog, usage: &RequestTokenStat) -> bool {
    (previous.model.is_none() && request.model.is_some())
        || [
            (previous.usage.input_tokens, usage.input_tokens),
            (
                previous.usage.cached_input_tokens,
                usage.cached_input_tokens,
            ),
            (previous.usage.output_tokens, usage.output_tokens),
            (previous.usage.total_tokens, usage.total_tokens),
            (
                previous.usage.reasoning_output_tokens,
                usage.reasoning_output_tokens,
            ),
        ]
        .iter()
        .any(|(old, new)| old.is_none() && new.is_some())
}

// 补充字段时双方已有计数必须一致；冲突回滚而不是拼成总数不自洽的记录。
fn ensureConsistentUsage(previous: &RequestTokenStat, incoming: &RequestTokenStat) -> Result<()> {
    if [
        (previous.input_tokens, incoming.input_tokens),
        (previous.cached_input_tokens, incoming.cached_input_tokens),
        (previous.output_tokens, incoming.output_tokens),
        (previous.total_tokens, incoming.total_tokens),
        (
            previous.reasoning_output_tokens,
            incoming.reasoning_output_tokens,
        ),
    ]
    .iter()
    .any(|(old, new)| old.zip(*new).is_some_and(|(old, new)| old != new))
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "同一响应的不同来源用量不一致".into(),
        ));
    }
    Ok(())
}

// 在同一事务查找新标识或旧版主机相关标识；仅匹配观测行，不合并普通网关账目。
fn loadPrevious(tx: &Transaction<'_>, trace: &str, aliases: &[String]) -> Result<Option<Previous>> {
    tx.query_row("SELECT r.id,r.request_type,r.model,t.input_tokens,t.cached_input_tokens,t.output_tokens,t.total_tokens,t.reasoning_output_tokens,
        (SELECT m.slug FROM request_charge_snapshots s JOIN models m ON m.id=s.model_id WHERE s.request_log_id=r.id),
        (SELECT s.base_cost_microusd FROM request_charge_snapshots s WHERE s.request_log_id=r.id),
        r.request_path,r.method,r.upstream_url,r.status_code,r.duration_ms,r.first_response_ms,r.error,r.created_at,r.model_source
        FROM request_logs r LEFT JOIN request_token_stats t ON t.request_log_id=r.id
        WHERE r.gateway_mode='directObservation' AND r.trace_id IN (?1,?2,?3) LIMIT 1",
        params![trace, aliases.first(), aliases.get(1)], |row| Ok(Previous {
            id: row.get(0)?, kind: row.get(1)?, model: row.get(2)?, usage: RequestTokenStat {
                input_tokens: row.get(3)?, cached_input_tokens: row.get(4)?, output_tokens: row.get(5)?,
                total_tokens: row.get(6)?, reasoning_output_tokens: row.get(7)?, ..Default::default()
            }, pricingModel: row.get(8)?, cost: row.get(9)?, route: RequestLog {
                request_type: row.get(1)?, model: row.get(2)?, request_path: row.get(10)?, method: row.get(11)?,
                upstream_url: row.get(12)?, status_code: row.get(13)?, duration_ms: row.get(14)?, first_response_ms: row.get(15)?,
                error: row.get(16)?, created_at: row.get(17)?, ..Default::default()
            }, modelSource: row.get::<_, Option<String>>(18)?.unwrap_or_else(|| "upstream".into())
        })).optional()
}

// 网络升级保留请求主键与路由元数据；持久化分流策略和会话 ID，避免列表把已分流请求显示成透传。
fn writeRequest(
    tx: &Transaction<'_>,
    request: &RequestLog,
    id: Option<i64>,
    modelSource: &str,
) -> Result<i64> {
    let client = request.request_type.as_deref() == Some(observationClientRequestType);
    tx.execute(
            "INSERT INTO request_logs (id, trace_id, request_path, original_path, method,
             request_type, gateway_mode, route_source, model, upstream_model, model_source,
             actual_source_kind, upstream_url, status_code, duration_ms, first_response_ms, error, created_at, reasoning_effort, service_tier, account_id, key_id, client_model, effective_service_tier, account_label, route_strategy, actual_source_id)
             VALUES (?12, ?1, ?2, ?2, ?3, ?4, 'directObservation', ?13, ?5, ?15,
             ?14, ?13, ?6, ?7, ?8, ?9, ?10, ?11, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
             ON CONFLICT(id) DO UPDATE SET trace_id=excluded.trace_id,request_path=excluded.request_path,
             original_path=excluded.original_path,method=excluded.method,request_type=excluded.request_type,
             route_source=excluded.route_source,model=excluded.model,upstream_model=excluded.upstream_model,
             model_source=excluded.model_source,actual_source_kind=excluded.actual_source_kind,
             upstream_url=excluded.upstream_url,status_code=excluded.status_code,duration_ms=excluded.duration_ms,
             first_response_ms=excluded.first_response_ms,error=excluded.error,created_at=excluded.created_at,reasoning_effort=COALESCE(excluded.reasoning_effort,request_logs.reasoning_effort),service_tier=COALESCE(excluded.service_tier,request_logs.service_tier),account_id=COALESCE(excluded.account_id,request_logs.account_id),key_id=COALESCE(excluded.key_id,request_logs.key_id),client_model=COALESCE(excluded.client_model,request_logs.client_model),effective_service_tier=COALESCE(excluded.effective_service_tier,request_logs.effective_service_tier),account_label=COALESCE(excluded.account_label,request_logs.account_label),route_strategy=COALESCE(excluded.route_strategy,request_logs.route_strategy),actual_source_id=COALESCE(excluded.actual_source_id,request_logs.actual_source_id)",
            params![request.trace_id, request.request_path, request.method, request.request_type,
                request.model, request.upstream_url, request.status_code, request.duration_ms,
                request.first_response_ms, request.error, request.created_at, id,
                if client { "clientObservation" } else { "directObservation" }, modelSource,
                if modelSource == "upstream" { request.model.as_deref() } else { None }, request.reasoning_effort, request.service_tier, request.account_id, request.key_id, request.client_model, request.effective_service_tier, request.account_label, request.route_strategy, request.actual_source_id],
        )?;
    let requestId = id.unwrap_or_else(|| tx.last_insert_rowid());
    // 同一账号且同一凭据指纹证明身份来源一致，可补齐旧网络记录名称；不猜测纯客户端事件的账号。
    if let (Some(label), Some(key), Some(account)) =
        (&request.account_label, &request.key_id, &request.account_id)
    {
        tx.execute("UPDATE request_logs SET account_label=?1 WHERE gateway_mode='directObservation' AND key_id=?2 AND account_id=?3 AND account_label IS NULL", params![label,key,account])?;
    }
    Ok(requestId)
}

// 只有网络采集提供新详情时才更新；客户端完成事件补录不清空原有正文。
fn saveDetails(tx: &Transaction<'_>, id: i64, details: Option<&str>) -> Result<()> {
    if let Some(details) = details {
        tx.execute("INSERT INTO request_details(request_log_id,payload) VALUES(?1,?2) ON CONFLICT(request_log_id) DO UPDATE SET payload=excluded.payload", params![id,details])?;
    }
    Ok(())
}
