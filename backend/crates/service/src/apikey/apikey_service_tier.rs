/// 函数 `normalize_service_tier`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn normalize_service_tier(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => None,
        "default" | "standard" => Some("default"),
        "fast" | "priority" => Some("fast"),
        "flex" => Some("flex"),
        "ultrafast" => Some("ultrafast"),
        _ => None,
    }
}

/// 函数 `normalize_service_tier_for_log`
///
///
/// 时间: 2026-04-05
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn normalize_service_tier_for_log(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => None,
        "default" | "standard" => Some("standard"),
        "fast" | "priority" => Some("fast"),
        "flex" => Some("flex"),
        "ultrafast" => Some("ultrafast"),
        _ => None,
    }
}

pub(crate) fn service_tier_request_matches_log_value(requested: &str, effective: &str) -> bool {
    normalize_service_tier_for_log(requested)
        .zip(normalize_service_tier_for_log(effective))
        .is_some_and(|(requested, effective)| requested == effective)
}

pub(crate) fn recover_omitted_standard_tier_for_log(
    effective_service_tier: Option<String>,
    api_key_service_tier: Option<&str>,
    client_service_tier: Option<&str>,
    model_policy_applied: bool,
) -> Option<String> {
    if model_policy_applied {
        return effective_service_tier;
    }
    effective_service_tier.or_else(|| {
        [api_key_service_tier, client_service_tier]
            .into_iter()
            .flatten()
            .find_map(|value| {
                (normalize_service_tier_for_log(value) == Some("standard"))
                    .then(|| "standard".to_string())
            })
    })
}
