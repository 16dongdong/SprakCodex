//! 直连观测只写请求与用量，按模型价格创建费用快照、不修改钱包或平台密钥额度。
use super::{ChargeSnapshotInputV2, RequestLog, RequestTokenStat, Storage};
use rusqlite::{params, Result};

#[allow(non_snake_case)]
impl Storage {
    // 在数据库工作线程内原子写入观测结果。trace_id 必须由观测器生成；
    // 相同响应重放返回 false，任何统计写入失败回滚整条请求，避免只保存半条记录。
    pub fn insertObservation(
        &self,
        request: &RequestLog,
        usage: &RequestTokenStat,
        pricingModel: Option<&str>,
    ) -> Result<bool> {
        let transaction = self.conn.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT INTO request_logs (trace_id, request_path, original_path, method,
             request_type, gateway_mode, route_source, model, upstream_model, model_source,
             actual_source_kind, upstream_url, status_code, duration_ms, first_response_ms, error, created_at)
             SELECT ?1, ?2, ?2, ?3, ?4, 'directObservation', 'directObservation', ?5, ?5,
             'upstream', 'directObservation', ?6, ?7, ?8, ?9, ?10, ?11
             WHERE NOT EXISTS (SELECT 1 FROM request_logs WHERE trace_id = ?1)",
            params![request.trace_id, request.request_path, request.method, request.request_type,
                request.model, request.upstream_url, request.status_code, request.duration_ms,
                request.first_response_ms, request.error, request.created_at],
        )?;
        if inserted == 0 {
            return Ok(false);
        }
        let requestId = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO request_token_stats (request_log_id, model, actual_source_kind,
             input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
             usage_included, created_at) VALUES (?1, ?2, 'directObservation', ?3, ?4, ?5, ?6, ?7, 1, ?8)",
            params![requestId, request.model, usage.input_tokens,
                usage.cached_input_tokens, usage.output_tokens, usage.total_tokens,
                usage.reasoning_output_tokens, request.created_at],
        )?;
        // 未知用量或缺失价格不写零价快照；保留可见原因供补齐价格后核对。
        let pricing = pricingModel
            .zip(usage.input_tokens)
            .zip(usage.output_tokens)
            .zip(usage.cached_input_tokens);
        let priced = match pricing {
            Some((((model, input), output), cached))
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
        if !priced {
            transaction.execute("UPDATE request_logs SET error=CASE WHEN error IS NULL THEN ?2 ELSE error || '；' || ?2 END WHERE id=?1",
                params![requestId, "费用未知：缺少完整用量或模型价格，未生成零价快照"])?;
        }
        transaction.commit()?;
        Ok(true)
    }
}
