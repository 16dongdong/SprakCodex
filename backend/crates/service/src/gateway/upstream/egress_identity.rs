use bytes::Bytes;
use codexmanager_core::storage::{AccountProxySettings, ProxyProfile, Storage};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const ACCEPT_LANGUAGE_HEADER: &str = "Accept-Language";
const OAI_LANGUAGE_HEADER: &str = "OAI-Language";
const CLIENT_TIMEZONE_HEADER: &str = "X-OpenAI-Client-Timezone";
const CLIENT_LOCALE_HEADER: &str = "X-OpenAI-Client-Locale";
const CLIENT_REGION_HEADER: &str = "X-OpenAI-Client-Region";
const GEO_ENDPOINT: &str = "https://ipwho.is/";
const GEO_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const GEO_FAILURE_TTL: Duration = Duration::from_secs(60);
const GEO_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EgressIdentity {
    pub(crate) timezone: String,
    pub(crate) locale: String,
    pub(crate) country: String,
    pub(crate) region: Option<String>,
    pub(crate) city: Option<String>,
}

#[derive(Clone)]
struct CachedIdentity {
    value: Option<EgressIdentity>,
    expires_at: Instant,
}

#[derive(Deserialize)]
struct GeoResponse {
    success: bool,
    country_code: Option<String>,
    region: Option<String>,
    city: Option<String>,
    timezone: Option<GeoTimezone>,
}

#[derive(Deserialize)]
struct GeoTimezone {
    id: Option<String>,
}

fn identity_cache() -> &'static Mutex<HashMap<String, CachedIdentity>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedIdentity>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 按本次账号实际启用的代理出口读取地理画像。
///
/// 该函数运行在上游候选已确定之后；仅使用代理健康检查已经写入 SQLite 的结果，避免在请求热路径发起额外网络探测。
/// 画像不完整时返回 `None`，调用方保持原请求，防止把猜测值写入线上协议。
pub(crate) fn resolve_for_account(account_id: &str) -> Option<EgressIdentity> {
    let storage = crate::storage_helpers::open_storage()?;
    resolve_from_storage(&storage, account_id).or_else(|| cached_identity(account_id))
}

/// 使用本次上游请求实际采用的 HTTP client 探测出口，因此系统 VPN、全局代理和账号代理都会得到同一路由结果。
/// 成功与失败均短期缓存，避免把地理服务延迟叠加到每个模型请求上。
pub(crate) fn resolve_for_account_with_client(
    account_id: &str,
    client: &Client,
    target_url: &str,
) -> Option<EgressIdentity> {
    if let Some(identity) = resolve_for_account(account_id) {
        return Some(identity);
    }
    if !is_official_target(target_url) {
        return None;
    }
    let identity = client
        .get(GEO_ENDPOINT)
        .timeout(GEO_REQUEST_TIMEOUT)
        .send()
        .ok()
        .filter(|response| response.status().is_success())
        .and_then(|response| response.json::<GeoResponse>().ok())
        .filter(|response| response.success)
        .and_then(|response| {
            build_identity(
                response
                    .timezone
                    .and_then(|timezone| timezone.id)
                    .as_deref(),
                response.country_code.as_deref(),
                response.region.as_deref(),
                response.city.as_deref(),
            )
        });
    cache_identity(account_id, identity.clone());
    identity
}

fn is_official_target(target_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(target_url) else {
        return false;
    };
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("chatgpt.com")
            || host.ends_with(".chatgpt.com")
            || host.eq_ignore_ascii_case("openai.com")
            || host.ends_with(".openai.com")
    })
}

fn cached_identity(account_id: &str) -> Option<EgressIdentity> {
    let now = Instant::now();
    let mut cache = identity_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cached = cache.get(account_id).cloned()?;
    if cached.expires_at <= now {
        cache.remove(account_id);
        return None;
    }
    cached.value
}

fn cache_identity(account_id: &str, identity: Option<EgressIdentity>) {
    let ttl = if identity.is_some() {
        GEO_CACHE_TTL
    } else {
        GEO_FAILURE_TTL
    };
    identity_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            account_id.to_string(),
            CachedIdentity {
                value: identity,
                expires_at: Instant::now() + ttl,
            },
        );
}

fn resolve_from_storage(storage: &Storage, account_id: &str) -> Option<EgressIdentity> {
    let settings = storage.find_account_proxy_settings(account_id).ok()??;
    if !settings.enabled {
        return None;
    }

    let uses_profile = match settings.proxy_source.as_deref().map(str::trim) {
        Some(source) if source.eq_ignore_ascii_case("profile") => true,
        Some(source) if source.eq_ignore_ascii_case("custom") => false,
        _ => settings
            .proxy_profile_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty()),
    };

    if uses_profile {
        let profile_id = settings.proxy_profile_id.as_deref()?.trim();
        let profile = storage.find_proxy_profile(profile_id).ok()??;
        if !profile.enabled {
            return None;
        }
        return identity_from_profile(&profile);
    }

    identity_from_settings(&settings)
}

fn identity_from_settings(settings: &AccountProxySettings) -> Option<EgressIdentity> {
    build_identity(
        settings.timezone_id.as_deref(),
        settings.country_code.as_deref(),
        settings.region_name.as_deref(),
        settings.city_name.as_deref(),
    )
}

fn identity_from_profile(profile: &ProxyProfile) -> Option<EgressIdentity> {
    build_identity(
        profile.timezone_id.as_deref(),
        profile.country_code.as_deref(),
        profile.region_name.as_deref(),
        profile.city_name.as_deref(),
    )
}

fn build_identity(
    timezone: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
) -> Option<EgressIdentity> {
    let timezone = normalize_text(timezone)?;
    let country = normalize_text(country)?.to_ascii_uppercase();
    if country.len() != 2 {
        return None;
    }
    Some(EgressIdentity {
        timezone,
        locale: locale_for_country(country.as_str()).to_string(),
        country,
        region: normalize_text(region),
        city: normalize_text(city),
    })
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn locale_for_country(country: &str) -> &'static str {
    match country {
        "US" => "en-US",
        "GB" => "en-GB",
        "CA" => "en-CA",
        "AU" => "en-AU",
        "NZ" => "en-NZ",
        "DE" => "de-DE",
        "FR" => "fr-FR",
        "ES" => "es-ES",
        "IT" => "it-IT",
        "PT" => "pt-PT",
        "BR" => "pt-BR",
        "JP" => "ja-JP",
        "KR" => "ko-KR",
        "CN" => "zh-CN",
        "TW" => "zh-TW",
        "HK" => "zh-HK",
        "RU" => "ru-RU",
        "IN" => "en-IN",
        "SG" => "en-SG",
        _ => "en-US",
    }
}

/// 用出口画像覆盖所有地区相关请求头，确保重试、HTTP 与 WebSocket 共用同一组稳定值。
pub(crate) fn apply_headers(headers: &mut Vec<(String, String)>, identity: &EgressIdentity) {
    for name in [
        ACCEPT_LANGUAGE_HEADER,
        OAI_LANGUAGE_HEADER,
        CLIENT_TIMEZONE_HEADER,
        CLIENT_LOCALE_HEADER,
        CLIENT_REGION_HEADER,
    ] {
        headers.retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
    }
    headers.push((
        ACCEPT_LANGUAGE_HEADER.to_string(),
        format!("{},en;q=0.9", identity.locale),
    ));
    headers.push((OAI_LANGUAGE_HEADER.to_string(), identity.locale.clone()));
    headers.push((
        CLIENT_TIMEZONE_HEADER.to_string(),
        identity.timezone.clone(),
    ));
    headers.push((CLIENT_LOCALE_HEADER.to_string(), identity.locale.clone()));
    headers.push((CLIENT_REGION_HEADER.to_string(), identity.country.clone()));
}

/// 将同一出口画像写入 Responses 的 `client_metadata`。
///
/// 请求头用于传输层识别，元数据用于会保留客户端上下文的 Codex HTTP/WS 协议；两处同时覆盖，避免原始中国地区值继续穿透。
pub(crate) fn apply_client_metadata(body: &Bytes, identity: &EgressIdentity) -> Bytes {
    let Ok(mut payload) = serde_json::from_slice::<Value>(body) else {
        return body.clone();
    };
    let Some(object) = payload.as_object_mut() else {
        return body.clone();
    };
    let metadata = object
        .entry("client_metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let Some(metadata) = metadata.as_object_mut() else {
        return body.clone();
    };
    metadata.insert(
        "timezone".to_string(),
        Value::String(identity.timezone.clone()),
    );
    metadata.insert("locale".to_string(), Value::String(identity.locale.clone()));
    metadata.insert(
        "country".to_string(),
        Value::String(identity.country.clone()),
    );
    if let Some(region) = identity.region.as_ref() {
        metadata.insert("region".to_string(), Value::String(region.clone()));
    }
    if let Some(city) = identity.city.as_ref() {
        metadata.insert("city".to_string(), Value::String(city.clone()));
    }
    serde_json::to_vec(&payload)
        .map(Bytes::from)
        .unwrap_or_else(|_| body.clone())
}

/// 为原生 Responses WebSocket 帧复用与 HTTP 完全相同的元数据改写规则。
pub(crate) fn apply_client_metadata_text(body: &str, identity: &EgressIdentity) -> String {
    let rewritten = apply_client_metadata(&Bytes::copy_from_slice(body.as_bytes()), identity);
    String::from_utf8(rewritten.to_vec()).unwrap_or_else(|_| body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_angeles_identity_overrides_headers_and_metadata() {
        let identity = EgressIdentity {
            timezone: "America/Los_Angeles".to_string(),
            locale: "en-US".to_string(),
            country: "US".to_string(),
            region: Some("California".to_string()),
            city: Some("Los Angeles".to_string()),
        };
        let mut headers = vec![("Accept-Language".to_string(), "zh-CN".to_string())];
        apply_headers(&mut headers, &identity);
        assert!(headers
            .iter()
            .any(|(name, value)| name == CLIENT_TIMEZONE_HEADER && value == "America/Los_Angeles"));
        assert_eq!(
            headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case(ACCEPT_LANGUAGE_HEADER))
                .count(),
            1
        );

        let body = Bytes::from_static(
            br#"{"model":"gpt-5.4","client_metadata":{"timezone":"Asia/Shanghai","locale":"zh-CN"}}"#,
        );
        let rewritten: Value =
            serde_json::from_slice(&apply_client_metadata(&body, &identity)).unwrap();
        assert_eq!(
            rewritten["client_metadata"]["timezone"],
            "America/Los_Angeles"
        );
        assert_eq!(rewritten["client_metadata"]["locale"], "en-US");
        assert_eq!(rewritten["client_metadata"]["country"], "US");
        assert_eq!(rewritten["client_metadata"]["region"], "California");
        assert_eq!(rewritten["client_metadata"]["city"], "Los Angeles");
    }
}
