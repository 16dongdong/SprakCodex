/// 函数 `normalize_service_tier`
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
/// 作者: gaohongshun
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

/// 函数 `normalize_service_tier_owned`
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
pub(crate) fn normalize_service_tier_owned(
    value: Option<String>,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => Ok(None),
        "default" | "standard" => Ok(Some("default".to_string())),
        "fast" | "priority" => Ok(Some("fast".to_string())),
        "flex" => Ok(Some("flex".to_string())),
        "ultrafast" => Ok(Some("ultrafast".to_string())),
        _ => Err(format!("unsupported service tier: {value}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_request_values_without_collapsing_ultrafast() {
        assert_eq!(normalize_service_tier("auto"), None);
        assert_eq!(normalize_service_tier("standard"), Some("default"));
        assert_eq!(normalize_service_tier("priority"), Some("fast"));
        assert_eq!(normalize_service_tier("flex"), Some("flex"));
        assert_eq!(normalize_service_tier("UltraFast"), Some("ultrafast"));
    }

    #[test]
    fn normalizes_service_tier_log_labels() {
        assert_eq!(normalize_service_tier_for_log("default"), Some("standard"));
        assert_eq!(normalize_service_tier_for_log("priority"), Some("fast"));
        assert_eq!(normalize_service_tier_for_log("flex"), Some("flex"));
        assert_eq!(
            normalize_service_tier_for_log("ultrafast"),
            Some("ultrafast")
        );
    }

    #[test]
    fn compares_wire_aliases_without_treating_auto_as_fast() {
        assert!(service_tier_request_matches_log_value("priority", "fast"));
        assert!(service_tier_request_matches_log_value(
            "default", "standard"
        ));
        assert!(!service_tier_request_matches_log_value("auto", "fast"));
        assert!(!service_tier_request_matches_log_value("auto", "standard"));
        assert!(!service_tier_request_matches_log_value("ultrafast", "fast"));
    }

    #[test]
    fn recovers_explicit_standard_when_the_wire_field_is_omitted() {
        assert_eq!(
            recover_omitted_standard_tier_for_log(None, Some("default"), Some("ultrafast"), false,)
                .as_deref(),
            Some("standard")
        );
        assert_eq!(
            recover_omitted_standard_tier_for_log(None, None, Some("default"), false).as_deref(),
            Some("standard")
        );
        assert_eq!(
            recover_omitted_standard_tier_for_log(None, Some("fast"), None, false),
            None
        );
        assert_eq!(
            recover_omitted_standard_tier_for_log(None, Some("default"), None, true),
            None,
            "a model policy that removed the wire field must not be logged as Standard"
        );
    }

    #[test]
    fn persisted_service_tiers_use_stable_canonical_values() {
        assert_eq!(
            normalize_service_tier_owned(Some("standard".to_string())),
            Ok(Some("default".to_string()))
        );
        assert_eq!(
            normalize_service_tier_owned(Some("priority".to_string())),
            Ok(Some("fast".to_string()))
        );
        assert_eq!(
            normalize_service_tier_owned(Some("ultrafast".to_string())),
            Ok(Some("ultrafast".to_string()))
        );
        assert!(normalize_service_tier_owned(Some("turbo".to_string())).is_err());
    }
}
