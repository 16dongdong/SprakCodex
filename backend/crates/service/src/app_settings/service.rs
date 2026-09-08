use super::{
    get_persisted_app_setting, normalize_optional_text, save_persisted_app_setting,
    APP_SETTING_SERVICE_ADDR_KEY,
};

pub const DEFAULT_ADDR: &str = "localhost:48760";
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:48760";
pub const DEFAULT_WEB_ADDR: &str = "localhost:48761";
pub const DEFAULT_WEB_BIND_ADDR: &str = "0.0.0.0:48761";
pub const SERVICE_BIND_MODE_SETTING_KEY: &str = "service.bind_mode";
pub const SERVICE_BIND_MODE_LOOPBACK: &str = "loopback";
pub const SERVICE_BIND_MODE_ALL_INTERFACES: &str = "all_interfaces";

/// 函数 `normalize_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - raw: 参数 raw
///
/// # 返回
/// 返回函数执行结果
/// 函数 `normalize_saved_service_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - raw: 参数 raw
///
/// # 返回
/// 返回函数执行结果
fn normalize_saved_service_addr(raw: Option<&str>) -> Result<String, String> {
    let Some(value) = normalize_optional_text(raw) else {
        return Ok(DEFAULT_ADDR.to_string());
    };
    let value = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(&value);
    let value = value.split('/').next().unwrap_or(value).trim();
    if value.is_empty() {
        return Err("service address is empty".to_string());
    }
    let port_text = value.rsplit_once(':').map_or(value, |(_, port)| port);
    let port = port_text
        .parse::<u16>()
        .map_err(|_| "service address port is invalid".to_string())?;
    Ok(format!("localhost:{port}"))
}

/// 函数 `current_env_service_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn current_env_service_addr() -> Option<String> {
    let raw = std::env::var("CODEXMANAGER_SERVICE_ADDR").ok()?;
    let normalized = normalize_saved_service_addr(Some(&raw)).ok()?;
    let Some((host, port)) = normalized.rsplit_once(':') else {
        return Some(normalized);
    };
    match host {
        "0.0.0.0" | "::" | "[::]" => Some(format!("localhost:{port}")),
        _ => Some(normalized),
    }
}

/// 函数 `current_env_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
/// 函数 `current_persisted_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
/// 函数 `current_effective_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn current_effective_service_bind_mode() -> String {
    SERVICE_BIND_MODE_LOOPBACK.to_string()
}

/// 函数 `current_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_service_bind_mode() -> String {
    SERVICE_BIND_MODE_LOOPBACK.to_string()
}

/// 函数 `set_service_bind_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - mode: 参数 mode
///
/// # 返回
/// 返回函数执行结果
pub fn set_service_bind_mode(mode: &str) -> Result<String, String> {
    // 个人版只允许本机回环监听；旧界面或旧数据库传入全接口模式时也统一收敛为 loopback。
    let _ = mode;
    let normalized = SERVICE_BIND_MODE_LOOPBACK.to_string();
    save_persisted_app_setting(SERVICE_BIND_MODE_SETTING_KEY, Some(&normalized))?;
    let current_addr = current_saved_service_addr();
    let synced_addr = listener_bind_addr_for_mode(&current_addr, &normalized);
    save_persisted_app_setting(APP_SETTING_SERVICE_ADDR_KEY, Some(&synced_addr))?;
    std::env::set_var("CODEXMANAGER_SERVICE_ADDR", &synced_addr);
    Ok(normalized)
}

/// 函数 `bind_all_interfaces_enabled`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn bind_all_interfaces_enabled() -> bool {
    false
}

/// 函数 `bind_all_interfaces_enabled_for_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - mode: 参数 mode
///
/// # 返回
/// 返回函数执行结果
pub fn bind_all_interfaces_enabled_for_mode(mode: &str) -> bool {
    let _ = mode;
    false
}

/// 函数 `default_listener_bind_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn default_listener_bind_addr() -> String {
    DEFAULT_ADDR.to_string()
}

/// 函数 `default_web_listener_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn default_web_listener_addr() -> String {
    let service_addr = current_saved_service_addr();
    let Some((_, port_text)) = service_addr.rsplit_once(':') else {
        return DEFAULT_WEB_ADDR.to_string();
    };
    let Ok(port) = port_text.parse::<u16>() else {
        return DEFAULT_WEB_ADDR.to_string();
    };
    format!("localhost:{}", port.saturating_add(1))
}

/// 函数 `listener_bind_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
pub fn listener_bind_addr(addr: &str) -> String {
    listener_bind_addr_for_mode(addr, &current_effective_service_bind_mode())
}

/// 函数 `listener_bind_addr_for_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - bind_mode: 参数 bind_mode
///
/// # 返回
/// 返回函数执行结果
pub fn listener_bind_addr_for_mode(addr: &str, bind_mode: &str) -> String {
    let _ = bind_mode;
    normalize_saved_service_addr(Some(addr)).unwrap_or_else(|_| DEFAULT_ADDR.to_string())
}

/// 函数 `current_saved_service_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_saved_service_addr() -> String {
    current_env_service_addr()
        .or_else(|| {
            get_persisted_app_setting(APP_SETTING_SERVICE_ADDR_KEY)
                .and_then(|value| normalize_saved_service_addr(Some(&value)).ok())
        })
        .unwrap_or_else(|| DEFAULT_ADDR.to_string())
}

/// 函数 `set_saved_service_addr`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
pub fn set_saved_service_addr(addr: Option<&str>) -> Result<String, String> {
    let normalized = normalize_saved_service_addr(addr)?;
    save_persisted_app_setting(APP_SETTING_SERVICE_ADDR_KEY, Some(&normalized))?;
    Ok(normalized)
}
