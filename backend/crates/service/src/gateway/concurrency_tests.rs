use super::recommend_gateway_concurrency;

/// 函数 `small_machine_prefers_conservative_values`
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
fn small_machine_prefers_conservative_values() {
    let recommendation = recommend_gateway_concurrency(2, 2_048);
    assert_eq!(recommendation.usage_refresh_workers, 2);
    assert_eq!(recommendation.http_worker_factor, 2);
    assert_eq!(recommendation.http_worker_min, 4);
    assert_eq!(recommendation.http_stream_worker_factor, 1);
    assert_eq!(recommendation.http_stream_worker_min, 1);
    assert_eq!(recommendation.account_max_inflight, 1);
}

/// 函数 `larger_machine_scales_up_gradually`
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
fn larger_machine_scales_up_gradually() {
    let recommendation = recommend_gateway_concurrency(16, 32_768);
    assert_eq!(recommendation.usage_refresh_workers, 6);
    assert_eq!(recommendation.http_worker_factor, 5);
    assert_eq!(recommendation.http_worker_min, 12);
    assert_eq!(recommendation.http_stream_worker_factor, 2);
    assert_eq!(recommendation.http_stream_worker_min, 4);
    assert_eq!(recommendation.account_max_inflight, 2);
}
