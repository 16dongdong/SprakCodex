/// 函数 `normalize_reasoning_effort`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn normalize_reasoning_effort(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" => Some("xhigh"),
        "max" => Some("max"),
        // 兼容历史写法；统一改写为官方使用的 xhigh，避免不同拼写在上游行为不一致。
        "extra_high" => Some("xhigh"),
        _ => None,
    }
}

/// 函数 `normalize_reasoning_effort_owned`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn normalize_reasoning_effort_owned(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_reasoning_effort)
        .map(str::to_string)
}

/// Codex 的 Ultra 是客户端编排模式；上游单次模型请求使用 Max 推理强度。
pub(crate) fn normalize_client_reasoning_effort_for_upstream(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ultra" => Some("max"),
        _ => normalize_reasoning_effort(value),
    }
}

pub(crate) fn is_ultra_to_max_normalization(
    client_value: Option<&str>,
    effective_value: Option<&str>,
) -> bool {
    client_value.is_some_and(|value| value.trim().eq_ignore_ascii_case("ultra"))
        && effective_value.is_some_and(|value| value.trim().eq_ignore_ascii_case("max"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_reasoning_effort_accepts_max_but_not_ultra() {
        assert_eq!(normalize_reasoning_effort(" MAX "), Some("max"));
        assert_eq!(normalize_reasoning_effort("ultra"), None);
    }

    #[test]
    fn client_ultra_is_preserved_as_a_distinct_input_but_sent_upstream_as_max() {
        assert_eq!(
            normalize_client_reasoning_effort_for_upstream(" Ultra "),
            Some("max")
        );
        assert!(is_ultra_to_max_normalization(Some("ultra"), Some("max")));
        assert!(!is_ultra_to_max_normalization(Some("max"), Some("max")));
    }
}
