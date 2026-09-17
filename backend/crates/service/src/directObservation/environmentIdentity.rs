//! 直连观测出口画像：探针与目标请求复用同一代理出口，并把变化实时发布给目标进程。

use cpcommon::relayContract::EnvironmentProfile;
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const openAiTraceEndpoint: &str = "https://chatgpt.com/cdn-cgi/trace";
const geoTimeout: Duration = Duration::from_secs(8);
pub(super) const probeInterval: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub(super) struct ObservedProfile {
    pub profile: EnvironmentProfile,
    pub edgeServer: String,
    pub edgeLocation: String,
    pub egressIp: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ProfileSnapshot {
    pub observed: ObservedProfile,
    pub updatedAt: i64,
    pub lastProbeAt: i64,
    pub lastProbeError: Option<String>,
}

pub(super) struct ProfileState {
    snapshot: RwLock<ProfileSnapshot>,
}

#[derive(Debug, Default)]
struct OpenAiEdgeTrace {
    ip: Option<String>,
    country: Option<String>,
    colo: Option<String>,
}

// 初次探针成功后建立共享快照；后续请求头、状态页与 DLL 配置始终读取这一来源。
impl ProfileState {
    pub fn new(observed: ObservedProfile) -> Self {
        let now = unixMilliseconds();
        Self {
            snapshot: RwLock::new(ProfileSnapshot {
                observed,
                updatedAt: now,
                lastProbeAt: now,
                lastProbeError: None,
            }),
        }
    }

    // 返回完整快照供状态页展示；锁损坏必须显式失败，不能继续展示过期值。
    pub fn snapshot(&self) -> Result<ProfileSnapshot, String> {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| "出口画像状态锁损坏".to_string())
    }

    // 请求转发只复制轻量画像，确保同一请求内所有地区头来自同一探针版本。
    pub fn profile(&self) -> Result<EnvironmentProfile, String> {
        self.snapshot().map(|snapshot| snapshot.observed.profile)
    }

    // 判断画像是否真正变化；探针时间变化不触发 DLL 配置重写。
    pub fn differs(&self, observed: &ObservedProfile) -> Result<bool, String> {
        self.snapshot
            .read()
            .map(|snapshot| {
                snapshot.observed.profile != observed.profile
                    || snapshot.observed.edgeServer != observed.edgeServer
                    || snapshot.observed.egressIp != observed.egressIp
            })
            .map_err(|_| "出口画像状态锁损坏".to_string())
    }

    // 只有探针及跨进程发布均成功后才提交新画像，避免界面与 DLL 看见不同版本。
    pub fn accept(&self, observed: ObservedProfile, changed: bool) -> Result<(), String> {
        let now = unixMilliseconds();
        let mut snapshot = self
            .snapshot
            .write()
            .map_err(|_| "出口画像状态锁损坏".to_string())?;
        snapshot.observed = observed;
        snapshot.lastProbeAt = now;
        snapshot.lastProbeError = None;
        if changed {
            snapshot.updatedAt = now;
        }
        Ok(())
    }

    // 探针失败只记录本轮错误，上一份已验证画像继续服务现有请求，下一周期仍会主动重试。
    pub fn reject(&self, error: String) -> Result<(), String> {
        let mut snapshot = self
            .snapshot
            .write()
            .map_err(|_| "出口画像状态锁损坏".to_string())?;
        snapshot.lastProbeAt = unixMilliseconds();
        snapshot.lastProbeError = Some(error);
        Ok(())
    }
}

// 每次都向 OpenAI 域名发送禁用缓存且主动断开连接的请求，代理切换后不会复用旧出口隧道。
pub(super) async fn resolve(client: &reqwest::Client) -> Result<ObservedProfile, String> {
    let response = client
        .get(openAiTraceEndpoint)
        .query(&[("probe", unixMilliseconds())])
        .header("accept", "text/plain")
        .header("cache-control", "no-cache, no-store")
        .header("pragma", "no-cache")
        .header("connection", "close")
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
    buildProfileFromEdge(parseOpenAiTrace(&body))
}

// Cloudflare trace 只解析公开出口字段；其它内容不进入画像，避免混入本机环境。
fn parseOpenAiTrace(body: &str) -> OpenAiEdgeTrace {
    let mut trace = OpenAiEdgeTrace::default();
    for line in body.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "ip" => trace.ip = normalized(Some(value.to_string())),
            "loc" => trace.country = normalized(Some(value.to_string())),
            "colo" => trace.colo = normalized(Some(value.to_string())),
            _ => {}
        }
    }
    trace
}

// 边缘机房映射使用实际 OpenAI 连接的 POP；不以本机区域或 UI 语言推断出口画像。
fn buildProfileFromEdge(trace: OpenAiEdgeTrace) -> Result<ObservedProfile, String> {
    let edgeServer = trace.colo.as_deref().ok_or("OpenAI 边缘探针未返回机房")?;
    let (country, timezone, locale, location) =
        edgeIdentity(edgeServer).ok_or_else(|| format!("OpenAI 边缘机房未映射：{edgeServer}"))?;
    let windowsTimezone =
        windowsTimezone(timezone).ok_or_else(|| format!("OpenAI 边缘时区未映射：{timezone}"))?;
    let observedCountry = trace.country.as_deref().unwrap_or(country);
    Ok(ObservedProfile {
        profile: EnvironmentProfile {
            country: observedCountry.to_string(),
            ianaTimezone: timezone.to_string(),
            windowsTimezone: windowsTimezone.to_string(),
            locale: locale.to_string(),
        },
        edgeServer: edgeServer.to_string(),
        edgeLocation: location.to_string(),
        egressIp: trace.ip,
    })
}

// 常见 OpenAI 边缘 POP 使用确定性地区映射；未知机房直接报错，避免把错误画像注入目标进程。
fn edgeIdentity(edge: &str) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    Some(match edge {
        "LAX" => ("US", "America/Los_Angeles", "en-US", "洛杉矶"),
        "SJC" => ("US", "America/Los_Angeles", "en-US", "圣何塞"),
        "SEA" => ("US", "America/Los_Angeles", "en-US", "西雅图"),
        "PHX" | "LAS" => ("US", "America/Phoenix", "en-US", "美国西部"),
        "DEN" => ("US", "America/Denver", "en-US", "丹佛"),
        "DFW" | "IAH" | "MCI" => ("US", "America/Chicago", "en-US", "美国中部"),
        "ORD" => ("US", "America/Chicago", "en-US", "芝加哥"),
        "IAD" | "EWR" | "JFK" | "ATL" | "MIA" | "BOS" => {
            ("US", "America/New_York", "en-US", "美国东部")
        }
        "YVR" => ("CA", "America/Vancouver", "en-CA", "温哥华"),
        "YYZ" => ("CA", "America/Toronto", "en-CA", "多伦多"),
        "GRU" => ("BR", "America/Sao_Paulo", "pt-BR", "圣保罗"),
        "LHR" => ("GB", "Europe/London", "en-GB", "伦敦"),
        "AMS" => ("NL", "Europe/Amsterdam", "nl-NL", "阿姆斯特丹"),
        "FRA" => ("DE", "Europe/Berlin", "de-DE", "法兰克福"),
        "CDG" => ("FR", "Europe/Paris", "fr-FR", "巴黎"),
        "MAD" => ("ES", "Europe/Madrid", "es-ES", "马德里"),
        "WAW" => ("PL", "Europe/Warsaw", "pl-PL", "华沙"),
        "NRT" | "KIX" => ("JP", "Asia/Tokyo", "ja-JP", "日本"),
        "ICN" => ("KR", "Asia/Seoul", "ko-KR", "首尔"),
        "SIN" => ("SG", "Asia/Singapore", "en-SG", "新加坡"),
        "HKG" => ("HK", "Asia/Hong_Kong", "zh-HK", "香港"),
        "TPE" => ("TW", "Asia/Taipei", "zh-TW", "台北"),
        "BOM" | "DEL" => ("IN", "Asia/Kolkata", "en-IN", "印度"),
        "DXB" => ("AE", "Asia/Dubai", "en-AE", "迪拜"),
        "SYD" | "MEL" => ("AU", "Australia/Sydney", "en-AU", "澳大利亚东部"),
        "HNL" => ("US", "Pacific/Honolulu", "en-US", "檀香山"),
        _ => return None,
    })
}

// 统一去掉服务端空白，空字符串视为缺失字段。
fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// IANA 到 Windows 键名沿用 CProxy 的目标进程画像契约；Windows 自身提供完整 DST 规则。
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

// 覆盖全部地区头；调用方传入单次快照，避免探针恰好更新时同一请求混用两个版本。
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

fn unixMilliseconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    // 固定样本同时验证 Windows API 键、出口信息与网络请求头来自同一画像。
    #[test]
    fn mapsLosAngelesProfileToWindowsAndHeaders() {
        let observed =
            buildProfileFromEdge(parseOpenAiTrace("ip=203.0.113.8\nloc=US\ncolo=LAX\n")).unwrap();
        assert_eq!(observed.profile.windowsTimezone, "Pacific Standard Time");
        assert_eq!(observed.profile.locale, "en-US");
        assert_eq!(observed.edgeLocation, "洛杉矶");
        assert_eq!(observed.egressIp.as_deref(), Some("203.0.113.8"));
        let mut headers = hyper::HeaderMap::new();
        applyHeaders(&mut headers, &observed.profile).unwrap();
        assert_eq!(headers["x-openai-client-timezone"], "America/Los_Angeles");
    }

    // 未知机房必须显式失败，禁止用本机时区或旧固定地区伪造探针结果。
    #[test]
    fn rejectsUnknownEdgeServer() {
        let error = buildProfileFromEdge(parseOpenAiTrace("loc=US\ncolo=UNKNOWN\n")).unwrap_err();
        assert!(error.contains("UNKNOWN"));
    }
}
