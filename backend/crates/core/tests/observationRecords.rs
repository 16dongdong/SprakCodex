#![allow(non_snake_case)]
use codexmanager_core::storage::{RequestLog, RequestTokenStat, Storage};

// 使用空数据库中的内置价格验证观测的费用事务，不接触用户数据库或真实请求。
#[test]
fn observationCreatesSnapshotWithoutWalletCharge() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    let request = RequestLog {
        trace_id: Some("observation:test".into()),
        request_path: "/v1/responses".into(),
        method: "POST".into(),
        model: Some("gpt-5.4-mini".into()),
        status_code: Some(200),
        created_at: 1,
        ..Default::default()
    };
    let usage = RequestTokenStat {
        input_tokens: Some(100),
        cached_input_tokens: Some(40),
        output_tokens: Some(20),
        total_tokens: Some(120),
        ..Default::default()
    };
    assert!(storage
        .insertObservation(&request, &usage, Some("gpt-5.4-mini"))
        .unwrap());
    assert!(!storage
        .insertObservation(&request, &usage, Some("gpt-5.4-mini"))
        .unwrap());
    let snapshot = storage.get_charge_snapshot_v2(1).unwrap().unwrap();
    assert_eq!(snapshot.input_tokens, 100);
    assert_eq!(snapshot.cached_input_tokens, 40);
    assert!(snapshot.base_cost_microusd > 0);
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
}

// 缺失价格不能被当成免费；未知费用不创建零价快照，原请求依旧保存且可按响应标识去重。
#[test]
fn missingPriceRemainsUnknown() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    let request = RequestLog {
        trace_id: Some("observation:unpriced".into()),
        request_path: "/v1/responses".into(),
        method: "POST".into(),
        model: Some("fixture-no-price".into()),
        created_at: 1,
        ..Default::default()
    };
    let usage = RequestTokenStat {
        input_tokens: Some(10),
        cached_input_tokens: Some(0),
        output_tokens: Some(2),
        ..Default::default()
    };
    assert!(storage
        .insertObservation(&request, &usage, Some("fixture-no-price"))
        .unwrap());
    assert!(storage.get_charge_snapshot_v2(1).unwrap().is_none());
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
}

// 官方 generate=false 预热不产生模型输出；保存其原始 usage 供核对，但生成趋势和费用估算不重复计算预热上下文。
#[test]
fn prewarmKeepsRawUsageWithoutGenerationCharge() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    let request = RequestLog {
        trace_id: Some("observation:prewarm".into()),
        request_path: "/backend-api/codex/responses".into(),
        request_type: Some(codexmanager_core::storage::observationPrewarmRequestType.into()),
        method: "POST".into(),
        model: Some("gpt-5.4-mini".into()),
        status_code: Some(200),
        created_at: 1,
        ..Default::default()
    };
    let usage = RequestTokenStat {
        input_tokens: Some(100),
        cached_input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(100),
        ..Default::default()
    };
    assert!(storage
        .insertObservation(&request, &usage, Some("gpt-5.4-mini"))
        .unwrap());
    let records = storage.list_request_logs(None, 10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, Some(100));
    assert!(records[0].error.is_none());
    assert!(storage.get_charge_snapshot_v2(1).unwrap().is_none());
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
    assert_eq!(
        storage
            .summarize_request_token_stats_between(0, 2)
            .unwrap()
            .input_tokens,
        0
    );
}

// 请求本身失败且费用未知时，组合诊断中的中文标点必须通过参数完整保留。
#[test]
fn combinedObservationDiagnosticPreservesUtf8() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    let request = RequestLog {
        trace_id: Some("observation:failed".into()),
        request_path: "/backend-api/codex/responses".into(),
        method: "GET".into(),
        status_code: Some(502),
        error: Some("上游握手失败".into()),
        created_at: 1,
        ..Default::default()
    };
    storage
        .insertObservation(&request, &RequestTokenStat::default(), None)
        .unwrap();
    let records = storage.list_request_logs(None, 1).unwrap();
    assert_eq!(
        records[0].error.as_deref(),
        Some("上游握手失败；费用未知：缺少完整用量或模型价格，未生成零价快照")
    );
}
