//! 直连观测出口画像：探测请求与目标请求共用同一 `reqwest::Client`，保证时区和区域来自真实出口。

use cpcommon::relayContract::EnvironmentProfile;
use std::time::Duration;

const openAiTraceEndpoint: &str = "https://chatgpt.com/cdn-cgi/trace";
const geoTimeout: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
struct OpenAiEdgeTrace {
    country: Option<String>,
    colo: Option<String>,
}

// 通过同一代理请求 OpenAI 边缘探针；colo 是实际连接到的边缘机房，不读取本机地区。
pub(super) async fn resolve(client: &reqwest::Client) -> Result<EnvironmentProfile, String> {
    let response = client
        .get(openAiTraceEndpoint)
        .header("accept", "text/plain")
        .timeout(geoTimeout)
        .send()
        .await
        .map_err(|error| format!("请求 OpenAI 边缘探针失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "请求 OpenAI 边缘探针失败：HTTP {}",
            response.status()
        ));
    }
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 OpenAI 边缘探针失败：{error}"))?;
    let trace = parseOpenAiTrace(&body);
    buildProfileFromEdge(trace)
}

// Cloudflare trace 只解析 loc/colo；其它字段不进入画像，避免把本机或代理环境变量混入结果。
fn parseOpenAiTrace(body: &str) -> OpenAiEdgeTrace {
    let mut trace = OpenAiEdgeTrace::default();
    for line in body.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "loc" => trace.country = normalized(Some(value.to_string())),
            "colo" => trace.colo = normalized(Some(value.to_string())),
            _ => {}
        }
    }
    trace
}

// 边缘机房到画像的确定性映射；LAX 明确代表洛杉矶，直接使用太平洋时区与英语区域。
fn buildProfileFromEdge(trace: OpenAiEdgeTrace) -> Result<EnvironmentProfile, String> {
    let (country, timezone, locale) = match trace.colo.as_deref().unwrap_or_default() {
        "LAX" => ("US", "America/Los_Angeles", "en-US"),
        "SJC" => ("US", "America/Los_Angeles", "en-US"),
        "SEA" => ("US", "America/Los_Angeles", "en-US"),
        "DFW" => ("US", "America/Chicago", "en-US"),
        "ORD" => ("US", "America/Chicago", "en-US"),
        "IAD" | "EWR" | "JFK" | "ATL" => ("US", "America/New_York", "en-US"),
        "AMS" => ("NL", "Europe/Amsterdam", "nl-NL"),
        "FRA" => ("DE", "Europe/Berlin", "de-DE"),
        "LHR" => ("GB", "Europe/London", "en-GB"),
        "NRT" => ("JP", "Asia/Tokyo", "ja-JP"),
        "ICN" => ("KR", "Asia/Seoul", "ko-KR"),
        "SIN" => ("SG", "Asia/Singapore", "en-SG"),
        "HKG" => ("HK", "Asia/Hong_Kong", "zh-HK"),
        "SYD" => ("AU", "Australia/Sydney", "en-AU"),
        _ => return Err(format!("OpenAI 边缘机房未映射：{:?}", trace.colo)),
    };
    let windowsTimezone =
        windowsTimezone(timezone).ok_or_else(|| format!("OpenAI 边缘时区未映射：{timezone}"))?;
    Ok(EnvironmentProfile {
        country: country.to_string(),
        ianaTimezone: timezone.to_string(),
        windowsTimezone: windowsTimezone.to_string(),
        locale: locale.to_string(),
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
        let profile = buildProfileFromEdge(parseOpenAiTrace("loc=US\ncolo=LAX\n")).unwrap();
        assert_eq!(profile.windowsTimezone, "Pacific Standard Time");
        assert_eq!(profile.locale, "en-US");
        let mut headers = hyper::HeaderMap::new();
        applyHeaders(&mut headers, &profile).unwrap();
        assert_eq!(headers["x-openai-client-timezone"], "America/Los_Angeles");
    }
}
