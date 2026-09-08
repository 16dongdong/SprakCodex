use super::*;
use crate::storage::RequestLog;

// 详情关联、重复写入、成员隔离和删除级联均在真实 SQLite 上验证。
#[test]
fn detailsFollowRecordOwnershipAndDeletion() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    let record = RequestLog {
        trace_id: Some("observation:detail-fixture".into()),
        request_path: "/v1/responses".into(),
        method: "POST".into(),
        key_id: Some("fixture-key".into()),
        ..Default::default()
    };
    storage
        .insertObservationDetails(
            &record,
            &Default::default(),
            crate::storage::ObservationContext {
                pricingModel: None,
                legacyTraces: &[],
                details: Some(r#"{"request":{},"response":{}}"#),
            },
        )
        .unwrap();
    let id: i64 = storage
        .conn
        .query_row(
            "SELECT id FROM request_logs WHERE trace_id='observation:detail-fixture'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(storage
        .readRequestDetails("observation:detail-fixture", None)
        .unwrap()
        .is_some());
    assert!(storage
        .readRequestDetails("observation:detail-fixture", Some(&["fixture-key".into()]))
        .unwrap()
        .is_some());
    assert!(storage
        .readRequestDetails(
            "observation:detail-fixture",
            Some(&["different-key".into()])
        )
        .unwrap()
        .is_none());
    storage
        .conn
        .execute("DELETE FROM request_logs WHERE id=?1", [id])
        .unwrap();
    let count: i64 = storage
        .conn
        .query_row("SELECT COUNT(*) FROM request_details", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

// 名称只回填相同账号与相同凭据指纹的旧记录，不把共享账号的其他凭据身份合并。
#[test]
fn accountLabelBackfillUsesExactFingerprint() {
    let storage = Storage::open_in_memory().unwrap();
    storage.init().unwrap();
    for (trace, key, label) in [
        ("observation:old", "direct:first", None),
        ("observation:other", "direct:other", None),
        (
            "observation:new",
            "direct:first",
            Some("Fixture <fixture@example.com>"),
        ),
    ] {
        storage
            .insertObservation(
                &RequestLog {
                    trace_id: Some(trace.into()),
                    key_id: Some(key.into()),
                    account_id: Some("account-fixture".into()),
                    account_label: label.map(str::to_owned),
                    request_path: "/v1/responses".into(),
                    method: "POST".into(),
                    ..Default::default()
                },
                &Default::default(),
                None,
                &[],
            )
            .unwrap();
    }
    let records = storage.list_request_logs(None, 10).unwrap();
    assert!(records
        .iter()
        .find(|record| record.trace_id.as_deref() == Some("observation:old"))
        .unwrap()
        .account_label
        .is_some());
    assert!(records
        .iter()
        .find(|record| record.trace_id.as_deref() == Some("observation:other"))
        .unwrap()
        .account_label
        .is_none());
}
