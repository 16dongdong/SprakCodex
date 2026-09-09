use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse, RequestLogListParams};
use codexmanager_core::storage::Storage;

use crate::storage_helpers::StorageHandle;
use crate::RpcActor;
use crate::{requestlog_clear, requestlog_list, requestlog_summary, requestlog_today_summary};

fn actor_key_ids_with_storage(storage: &Storage, actor: &RpcActor) -> Result<Vec<String>, String> {
    if actor.is_admin() {
        return Ok(Vec::new());
    }
    let user_id = actor
        .user_id
        .as_deref()
        .ok_or_else(|| "permission_denied: requestlog requires user session".to_string())?;
    storage
        .list_api_key_ids_for_user(user_id)
        .map_err(|err| format!("list api key ids for user failed: {err}"))
}

fn member_requestlog_scope(actor: &RpcActor) -> Result<(StorageHandle, Vec<String>), String> {
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let key_ids = actor_key_ids_with_storage(&storage, actor)?;
    Ok((storage, key_ids))
}

/// 函数 `try_handle`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn try_handle(req: &JsonRpcRequest, actor: &RpcActor) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        // 桶边界由浏览器本地日历生成；服务端限制数量和跨度，复用含归档数据的统计而非截断请求列表。
        "requestlog/tokenSeries" => super::value_or_error((|| {
            if !actor.is_admin() { return Err("permission_denied".to_string()); }
            let boundaries: Vec<i64> = serde_json::from_value(req.params.as_ref().and_then(|p| p.get("boundaries")).cloned().ok_or("缺少时间边界")?).map_err(|_| "时间边界格式无效")?;
            if boundaries.len() < 2 || boundaries.len() > 33 || boundaries[0] < 0 || boundaries.windows(2).any(|pair| pair[0] >= pair[1]) || boundaries[boundaries.len()-1].saturating_sub(boundaries[0]) > 32 * 86400 {
                return Err("时间边界范围无效".to_string());
            }
            let storage = crate::storage_helpers::open_storage().ok_or("打开存储失败")?;
            boundaries.windows(2).map(|pair| {
                let usage = storage.summarize_request_logs_between(pair[0], pair[1]).map_err(|error| error.to_string())?;
                Ok(serde_json::json!({"time":pair[0] * 1000,"tokens":usage.input_tokens.saturating_add(usage.output_tokens)}))
            }).collect::<Result<Vec<_>, String>>()
        })()),
        // 个人累计费用只向管理员开放，历史快照分项不通过客户端猜算。
        "requestlog/costBreakdown" => super::value_or_error((|| {
            if !actor.is_admin() { return Err("permission_denied".to_string()); }
            let storage = crate::storage_helpers::open_storage().ok_or("打开存储失败")?;
            let cost = storage.cumulativeCostBreakdown().map_err(|error| error.to_string())?;
            // 用量从累计统计读取，包含归档汇总；缓存是输入子集，拆分时只扣除一次。
            let usage = storage.summarize_request_logs_between(0, i64::MAX).map_err(|error| error.to_string())?;
            let cache = usage.cached_input_tokens.min(usage.input_tokens).max(0);
            let input = usage.input_tokens.saturating_sub(cache).max(0);
            let output = usage.output_tokens.max(0);
            let mut result = serde_json::to_value(cost).map_err(|error| error.to_string())?;
            result["tokens"] = serde_json::json!({"input":input,"output":output,"cache":cache,"total":input.saturating_add(cache).saturating_add(output)});
            Ok(result)
        })()),
        // 详情沿用列表的不透明 traceId，权限与列表同源；历史未采集返回 null，不把摘要当报文。
        "requestlog/detail" => super::value_or_error((|| {
            let trace = req
                .params
                .as_ref()
                .and_then(|params| params.get("traceId"))
                .and_then(|trace| trace.as_str())
                .filter(|trace| !trace.is_empty() && trace.len() <= 256)
                .ok_or("请求记录 ID 无效")?;
            let storage = crate::storage_helpers::open_storage().ok_or("打开存储失败")?;
            let keys = actor_key_ids_with_storage(&storage, actor)?;
            let payload = storage
                .readRequestDetails(trace, if actor.is_admin() { None } else { Some(&keys) })
                .map_err(|error| error.to_string())?;
            payload
                .map(|text| {
                    serde_json::from_str::<serde_json::Value>(&text)
                        .map_err(|_| "请求详情格式无效".to_owned())
                })
                .transpose()
        })()),
        "requestlog/list_with_summary" => {
            let params = req
                .params
                .clone()
                .map(serde_json::from_value::<RequestLogListParams>)
                .transpose()
                .map(|params| params.unwrap_or_default())
                .map(RequestLogListParams::normalized)
                .map_err(|err| format!("invalid requestlog/list_with_summary params: {err}"));
            super::value_or_error(params.and_then(|params| {
                if actor.is_admin() {
                    let storage = crate::storage_helpers::open_storage()
                        .ok_or_else(|| "open storage failed".to_string())?;
                    let summary =
                        requestlog_summary::read_request_log_filter_summary_with_storage(
                            &storage,
                            params.clone(),
                        )?;
                    let page = if requestlog_list::request_log_page_total_matches_filter_summary(
                        &params,
                    ) {
                        requestlog_list::read_request_log_page_with_total(
                            &storage,
                            params,
                            summary.filtered_count,
                        )?
                    } else {
                        requestlog_list::read_request_log_page_with_storage(&storage, params)?
                    };
                    Ok(requestlog_list::request_log_list_with_summary_result(
                        page, summary,
                    ))
                } else {
                    let (storage, key_ids) = member_requestlog_scope(actor)?;
                    let summary =
                        requestlog_summary::read_request_log_filter_summary_for_key_ids_with_storage(
                            &storage,
                            params.clone(),
                            &key_ids,
                        )?;
                    let page = if requestlog_list::request_log_page_total_matches_filter_summary(
                        &params,
                    ) {
                        requestlog_list::read_request_log_page_for_key_ids_with_total(
                            &storage,
                            params,
                            &key_ids,
                            summary.filtered_count,
                        )?
                    } else {
                        requestlog_list::read_request_log_page_for_key_ids_with_storage(
                            &storage, params, &key_ids,
                        )?
                    };
                    Ok(requestlog_list::request_log_list_with_summary_result(
                        page, summary,
                    ))
                }
            }))
        }
        "requestlog/list" => {
            let params = req
                .params
                .clone()
                .map(serde_json::from_value::<RequestLogListParams>)
                .transpose()
                .map(|params| params.unwrap_or_default())
                .map(RequestLogListParams::normalized)
                .map_err(|err| format!("invalid requestlog/list params: {err}"));
            super::value_or_error(params.and_then(|params| {
                if actor.is_admin() {
                    requestlog_list::read_request_log_page(params)
                } else {
                    let (storage, key_ids) = member_requestlog_scope(actor)?;
                    requestlog_list::read_request_log_page_for_key_ids_with_storage(
                        &storage, params, &key_ids,
                    )
                }
            }))
        }
        "requestlog/summary" => {
            let params = req
                .params
                .clone()
                .map(serde_json::from_value::<RequestLogListParams>)
                .transpose()
                .map(|params| params.unwrap_or_default())
                .map(RequestLogListParams::normalized)
                .map_err(|err| format!("invalid requestlog/summary params: {err}"));
            super::value_or_error(params.and_then(|params| {
                if actor.is_admin() {
                    requestlog_summary::read_request_log_filter_summary(params)
                } else {
                    let (storage, key_ids) = member_requestlog_scope(actor)?;
                    requestlog_summary::read_request_log_filter_summary_for_key_ids_with_storage(
                        &storage, params, &key_ids,
                    )
                }
            }))
        }
        "requestlog/clear" => super::ok_or_error(requestlog_clear::clear_request_logs()),
        "requestlog/today_summary" => {
            let day_start_ts = super::i64_param(req, "dayStartTs");
            let day_end_ts = super::i64_param(req, "dayEndTs");
            super::value_or_error(if actor.is_admin() {
                requestlog_today_summary::read_requestlog_today_summary(day_start_ts, day_end_ts)
            } else {
                member_requestlog_scope(actor).and_then(|(storage, key_ids)| {
                    requestlog_today_summary::read_requestlog_today_summary_for_key_ids_with_storage(
                        &storage,
                        day_start_ts,
                        day_end_ts,
                        &key_ids,
                    )
                })
            })
        }
        _ => return None,
    };

    Some(super::response(req, result))
}
