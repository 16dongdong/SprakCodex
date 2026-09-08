pub(crate) const PROTOCOL_OPENAI_COMPAT: &str = "openai_compat";
pub(crate) const PROTOCOL_ANTHROPIC_NATIVE: &str = "anthropic_native";
pub(crate) const PROTOCOL_GEMINI_NATIVE: &str = "gemini_native";
pub(crate) const ROTATION_AGGREGATE_API: &str = "aggregate_api_rotation";
pub(crate) const ROTATION_HYBRID: &str = "hybrid_rotation";
pub(crate) const ROTATION_HYBRID_AGGREGATE_FIRST: &str = "hybrid_aggregate_first_rotation";
#[cfg(test)]
pub(crate) const ROTATION_ACCOUNT: &str = "account_rotation";

/// 函数 `normalize_key`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

/// 函数 `is_anthropic_request_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn is_anthropic_request_path(path: &str) -> bool {
    path == "/v1/messages" || path.starts_with("/v1/messages/") || path.starts_with("/v1/messages?")
}

fn normalized_request_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn is_gemini_internal_generate_content_request_path(path: &str) -> bool {
    matches!(
        normalized_request_path(path),
        "/v1internal:generateContent" | "/v1internal:streamGenerateContent"
    )
}

pub(crate) fn is_gemini_generate_content_request_path(path: &str) -> bool {
    let normalized = normalized_request_path(path);
    if is_gemini_internal_generate_content_request_path(normalized) {
        return true;
    }
    ["/v1/models/", "/v1beta/models/", "/v1alpha/models/"]
        .iter()
        .any(|prefix| {
            normalized.starts_with(prefix)
                && (normalized.contains(":generateContent")
                    || normalized.contains(":streamGenerateContent"))
        })
}

pub(crate) fn is_gemini_count_tokens_request_path(path: &str) -> bool {
    let normalized = normalized_request_path(path);
    if normalized == "/v1internal:countTokens" {
        return true;
    }
    ["/v1/models/", "/v1beta/models/", "/v1alpha/models/"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix) && normalized.contains(":countTokens"))
}

pub(crate) fn is_gemini_request_path(path: &str) -> bool {
    is_gemini_generate_content_request_path(path) || is_gemini_count_tokens_request_path(path)
}

/// 函数 `resolve_gateway_protocol_type`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// - protocol_type: 参数 protocol_type
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn resolve_gateway_protocol_type(protocol_type: &str, path: &str) -> &'static str {
    match normalize_key(protocol_type).as_str() {
        _ if is_gemini_request_path(path) => PROTOCOL_GEMINI_NATIVE,
        // 中文注释：平台 Key 对 Codex / Claude Code 默认按路径通配；
        // `/v1/messages*` 走 Claude 语义，Gemini 原生路径走 Gemini 语义，其余标准路径走 OpenAI/Codex 语义。
        _ if is_anthropic_request_path(path) => PROTOCOL_ANTHROPIC_NATIVE,
        _ => PROTOCOL_OPENAI_COMPAT,
    }
}
