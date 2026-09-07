use super::{
    derive_canonical_source, derive_size_reject_stage, normalize_optional_text,
    normalize_status_filter, normalize_upstream_url,
    read_request_log_page_for_key_ids_with_storage, read_request_logs_for_key_ids_with_storage,
    read_request_logs_with_storage, request_log_page_total_matches_filter_summary,
    RequestLogListParams, DEFAULT_REQUEST_LOG_PAGE_SIZE,
};
use codexmanager_core::storage::Storage;

/// 函数 `normalize_upstream_url_keeps_official_domains`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_upstream_url_keeps_official_domains() {
    assert_eq!(
        normalize_upstream_url(Some("https://chatgpt.com/backend-api/codex/responses")).as_deref(),
        Some("https://chatgpt.com/backend-api/codex/responses")
    );
    assert_eq!(
        normalize_upstream_url(Some("https://api.openai.com/v1/responses")).as_deref(),
        Some("https://api.openai.com/v1/responses")
    );
}

/// 函数 `normalize_upstream_url_keeps_local_addresses`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_upstream_url_keeps_local_addresses() {
    assert_eq!(
        normalize_upstream_url(Some("http://127.0.0.1:3000/relay")).as_deref(),
        Some("http://127.0.0.1:3000/relay")
    );
    assert_eq!(
        normalize_upstream_url(Some("http://localhost:3000/relay")).as_deref(),
        Some("http://localhost:3000/relay")
    );
}

/// 函数 `normalize_upstream_url_keeps_custom_addresses`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_upstream_url_keeps_custom_addresses() {
    assert_eq!(
        normalize_upstream_url(Some("https://gateway.example.com/v1")).as_deref(),
        Some("https://gateway.example.com/v1")
    );
}

/// 函数 `normalize_upstream_url_trims_empty_values`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_upstream_url_trims_empty_values() {
    assert_eq!(normalize_upstream_url(None), None);
    assert_eq!(normalize_upstream_url(Some("   ")), None);
    assert_eq!(
        normalize_upstream_url(Some(" https://api.openai.com/v1/responses ")).as_deref(),
        Some("https://api.openai.com/v1/responses")
    );
}

#[test]
fn derive_canonical_source_uses_adapter_and_aggregate_context() {
    assert_eq!(
        derive_canonical_source(Some("Passthrough"), None, None, &[]),
        "native_codex"
    );
    assert_eq!(
        derive_canonical_source(Some("OpenAIChatCompletionsSse"), None, None, &[]),
        "openai_compat"
    );
    assert_eq!(
        derive_canonical_source(Some("AnthropicSse"), None, None, &[]),
        "anthropic_adapter"
    );
    assert_eq!(
        derive_canonical_source(Some("GeminiJson"), None, None, &[]),
        "gemini_adapter"
    );
    assert_eq!(
        derive_canonical_source(
            Some("Passthrough"),
            Some("supplier"),
            None,
            &["agg-1".to_string()],
        ),
        "aggregate_passthrough"
    );
}

#[test]
fn derive_size_reject_stage_distinguishes_local_and_upstream() {
    assert_eq!(
        derive_size_reject_stage(
            Some(400),
            Some("Input exceeds the maximum length of 1048576 characters."),
        ),
        "local"
    );
    assert_eq!(
        derive_size_reject_stage(Some(413), Some("upstream request body too large")),
        "upstream"
    );
    assert_eq!(derive_size_reject_stage(Some(413), None), "upstream");
    assert_eq!(derive_size_reject_stage(Some(200), None), "-");
}

/// 函数 `request_log_list_params_default_to_first_page_with_twenty_items`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn request_log_list_params_default_to_first_page_with_twenty_items() {
    let params: RequestLogListParams =
        serde_json::from_value(serde_json::json!({})).expect("deserialize params");
    let normalized = params.normalized();

    assert_eq!(normalized.page, 1);
    assert_eq!(normalized.page_size, DEFAULT_REQUEST_LOG_PAGE_SIZE);
}

/// 函数 `normalize_status_filter_accepts_known_values`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_status_filter_accepts_known_values() {
    assert_eq!(
        normalize_status_filter(Some("2xx".to_string())).as_deref(),
        Some("2xx")
    );
    assert_eq!(normalize_status_filter(Some("ALL".to_string())), None);
    assert_eq!(normalize_status_filter(Some("unknown".to_string())), None);
}

/// 函数 `normalize_optional_text_trims_blank_values`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn normalize_optional_text_trims_blank_values() {
    assert_eq!(normalize_optional_text(Some("  ".to_string())), None);
    assert_eq!(
        normalize_optional_text(Some(" trace:=abc ".to_string())).as_deref(),
        Some("trace:=abc")
    );
}

#[test]
fn request_log_page_total_matches_filter_summary_only_for_log_filters() {
    assert!(!request_log_page_total_matches_filter_summary(
        &RequestLogListParams::default()
    ));
    assert!(!request_log_page_total_matches_filter_summary(
        &RequestLogListParams {
            status_filter: Some("all".to_string()),
            query: Some("   ".to_string()),
            ..RequestLogListParams::default()
        }
    ));
    assert!(request_log_page_total_matches_filter_summary(
        &RequestLogListParams {
            status_filter: Some("5xx".to_string()),
            ..RequestLogListParams::default()
        }
    ));
    assert!(request_log_page_total_matches_filter_summary(
        &RequestLogListParams {
            query: Some("trace:=abc".to_string()),
            ..RequestLogListParams::default()
        }
    ));
}

#[test]
fn member_request_log_reads_short_circuit_empty_key_ids() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let items = read_request_logs_for_key_ids_with_storage(&storage, None, Some(20), &[])
        .expect("read empty member logs");
    assert!(items.is_empty());

    let page = read_request_log_page_for_key_ids_with_storage(
        &storage,
        RequestLogListParams {
            page: 2,
            page_size: 50,
            ..RequestLogListParams::default()
        },
        &[],
    )
    .expect("read empty member log page");

    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.page, 1);
    assert_eq!(page.page_size, 50);
}

#[test]
fn request_log_summary_reads_short_circuit_zero_limit() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let items =
        read_request_logs_with_storage(&storage, None, Some(0)).expect("read empty admin logs");
    assert!(items.is_empty());

    let member_items =
        read_request_logs_for_key_ids_with_storage(&storage, None, Some(0), &["key-1".to_string()])
            .expect("read empty member logs");
    assert!(member_items.is_empty());
}
