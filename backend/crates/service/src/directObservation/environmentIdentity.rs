//! 直连观测出口画像：探测请求与目标请求共用同一 `reqwest::Client`，保证时区和区域来自真实出口。

use cpcommon::relayContract::EnvironmentProfile;
use serde::Deserialize;
use std::time::Duration;

const geoEndpoint: &str = "https://ipwho.is/";
const geoTimeout: Duration = Duration::from_secs(5);

// 地理服务响应只解析形成目标画像所需字段，避免把无关网络信息带入进程协议。
#[derive(Deserialize)]
struct GeoResponse {
    success: bool,
    country_code: Option<String>,
    timezone: Option<GeoTimezone>,
}

// 时区子对象使用 IANA 标识，Windows 键名由本地确定性映射产生。
#[derive(Deserialize)]
struct GeoTimezone {
    id: Option<String>,
}

// 通过已经绑定当前出口的客户端读取地区画像；字段缺失或时区无法准确映射时直接报错，避免发布混合身份。
pub(super) async fn resolve(client: &reqwest::Client) -> Result<EnvironmentProfile, String> {
    let response = client
        .get(geoEndpoint)
        .timeout(geoTimeout)
        .send()
        .await
        .map_err(|error| format!("探测观测出口地区失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("探测观测出口地区失败：HTTP {}", response.status()));
    }
    let payload = response
        .json::<GeoResponse>()
        .await
        .map_err(|error| format!("解析观测出口地区失败：{error}"))?;
    buildProfile(payload)
}

// 将网络响应收敛为共享协议；Windows 时区键必须明确存在，不能把未知 IANA 时区猜成宿主时区。
fn buildProfile(payload: GeoResponse) -> Result<EnvironmentProfile, String> {
    if !payload.success {
        return Err("探测观测出口地区失败：服务返回 success=false".into());
    }
    let country = normalized(payload.country_code)
        .map(|value| value.to_ascii_uppercase())
        .filter(|value| value.len() == 2)
        .ok_or("观测出口国家码缺失")?;
    let ianaTimezone =
        normalized(payload.timezone.and_then(|timezone| timezone.id)).ok_or("观测出口时区缺失")?;
    let windowsTimezone = windowsTimezone(ianaTimezone.as_str())
        .ok_or_else(|| format!("观测出口时区暂未映射到 Windows：{ianaTimezone}"))?;
    Ok(EnvironmentProfile {
        locale: localeForCountry(country.as_str()).to_string(),
        country,
        ianaTimezone,
        windowsTimezone: windowsTimezone.to_string(),
    })
}

// 统一去掉服务端空白，空字符串视为缺失字段。
fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// IANA 到 Windows 键名沿用 CProxy 的目标进程画像契约；相同键由 Windows 自身提供完整 DST 规则。
fn windowsTimezone(iana: &str) -> Option<&'static str> {
    Some(match iana {
        "America/Los_Angeles" | "America/Vancouver" | "America/Tijuana" => "Pacific Standard Time",
        "America/Denver" | "America/Edmonton" => "Mountain Standard Time",
        "America/Phoenix" => "US Mountain Standard Time",
        "America/Chicago" | "America/Winnipeg" | "America/Mexico_City" => "Central Standard Time",
        "America/New_York" | "America/Toronto" => "Eastern Standard Time",
        "America/Anchorage" => "Alaskan Standard Time",
        "Pacific/Honolulu" => "Hawaiian Standard Time",
        "America/Sao_Paulo" => "E. South America Standard Time",
        "Europe/London" | "Europe/Dublin" | "Europe/Lisbon" => "GMT Standard Time",
        "Europe/Paris" | "Europe/Madrid" | "Europe/Brussels" => "Romance Standard Time",
        "Europe/Berlin" | "Europe/Amsterdam" | "Europe/Rome" | "Europe/Vienna"
        | "Europe/Zurich" => "W. Europe Standard Time",
        "Europe/Warsaw" | "Europe/Stockholm" | "Europe/Prague" => "Central European Standard Time",
        "Europe/Moscow" => "Russian Standard Time",
        "Asia/Tokyo" => "Tokyo Standard Time",
        "Asia/Seoul" => "Korea Standard Time",
        "Asia/Shanghai" | "Asia/Hong_Kong" | "Asia/Macau" => "China Standard Time",
        "Asia/Singapore" | "Asia/Kuala_Lumpur" => "Singapore Standard Time",
        "Asia/Taipei" => "Taipei Standard Time",
        "Asia/Kolkata" | "Asia/Calcutta" => "India Standard Time",
        "Asia/Dubai" => "Arabian Standard Time",
        "Australia/Sydney" | "Australia/Melbourne" => "AUS Eastern Standard Time",
        "UTC" | "Etc/UTC" | "Etc/GMT" => "UTC",
        _ => return None,
    })
}

// 国家码只决定地区语言，不改变目标进程的操作系统与硬件字段。
fn localeForCountry(country: &str) -> &'static str {
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

// 覆盖所有地区相关头；同一请求只保留一份值，避免客户端旧值与出口画像同时出现。
pub(super) fn applyHeaders(
    headers: &mut hyper::HeaderMap,
    profile: &EnvironmentProfile,
) -> Result<(), String> {
    use hyper::header::{HeaderName, HeaderValue, ACCEPT_LANGUAGE};
    for name in [
        "oai-language",
        "x-openai-client-timezone",
        "x-openai-client-locale",
        "x-openai-client-region",
    ] {
        headers.remove(name);
    }
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_str(format!("{},en;q=0.9", profile.locale).as_str())
            .map_err(|_| "出口区域语言不是有效请求头".to_string())?,
    );
    for (name, value) in [
        ("oai-language", profile.locale.as_str()),
        ("x-openai-client-timezone", profile.ianaTimezone.as_str()),
        ("x-openai-client-locale", profile.locale.as_str()),
        ("x-openai-client-region", profile.country.as_str()),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(value)
                .map_err(|_| format!("出口画像字段无法写入请求头：{name}"))?,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 固定样本同时验证 Windows API 键与网络请求头来自同一画像。
    #[test]
    fn mapsLosAngelesProfileToWindowsAndHeaders() {
        let profile = buildProfile(GeoResponse {
            success: true,
            country_code: Some("us".into()),
            timezone: Some(GeoTimezone {
                id: Some("America/Los_Angeles".into()),
            }),
        })
        .unwrap();
        assert_eq!(profile.windowsTimezone, "Pacific Standard Time");
        assert_eq!(profile.locale, "en-US");
        let mut headers = hyper::HeaderMap::new();
        applyHeaders(&mut headers, &profile).unwrap();
        assert_eq!(headers["x-openai-client-timezone"], "America/Los_Angeles");
    }
}
