//! 内置代理运行时：持久化完整节点配置，管理 mihomo 子进程，并把 mixed-port 发布给现有网关。

pub mod config;
pub mod kernel;

use config::{ProxyConfig, ProxyNode};
use kernel::{Kernel, NodeDelay};
use serde::Serialize;
use std::sync::{Mutex, OnceLock};

const proxyConfigSettingKey: &str = "proxy.runtime.configuration";
const maxSubscriptionBytes: usize = 8 * 1024 * 1024;
static kernelRuntime: OnceLock<Mutex<Option<Kernel>>> = OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeStatus {
    pub running: bool,
    pub mixedPort: Option<u16>,
    pub active: Option<String>,
    pub nodeCount: usize,
    pub groupCount: usize,
    pub subscriptionCount: usize,
}

#[derive(Debug, Serialize)]
pub struct EgressView {
    pub via: String,
    pub ip: String,
    pub country: String,
    pub city: String,
    pub isp: String,
    pub error: Option<String>,
}

fn runtime() -> &'static Mutex<Option<Kernel>> {
    kernelRuntime.get_or_init(|| Mutex::new(None))
}

// 从应用数据库读取完整 Clash 配置；缺失配置返回关闭状态，反序列化失败直接报告。
pub fn loadConfig() -> Result<ProxyConfig, String> {
    Ok(loadStoredConfig()?.unwrap_or_default())
}

fn loadStoredConfig() -> Result<Option<ProxyConfig>, String> {
    crate::storage_helpers::initialize_storage()?;
    let storage = crate::storage_helpers::open_storage().ok_or("打开代理配置存储失败")?;
    let Some(encoded) = storage
        .get_app_setting(proxyConfigSettingKey)
        .map_err(|error| format!("读取代理配置失败：{error}"))?
    else {
        return Ok(None);
    };
    serde_json::from_str(&encoded)
        .map(Some)
        .map_err(|error| format!("解析代理配置失败：{error}"))
}

// 保存前校验活动出口，防止开关启用后生成指向不存在节点的 GLOBAL 选择器。
fn validateConfig(config: &ProxyConfig) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }
    let active = config
        .active
        .as_deref()
        .filter(|active| !active.trim().is_empty())
        .ok_or("开启代理前请选择出口节点或代理组")?;
    let exists = config.nodes.iter().any(|node| node.name == active)
        || config.groups.iter().any(|group| group.name == active);
    if !exists {
        return Err(format!("当前出口不存在：{active}"));
    }
    Ok(())
}

// 数据库是配置真源；写入成功后再热重载内核，失败时恢复上一配置及运行链路。
pub fn saveConfig(config: ProxyConfig) -> Result<ProxyConfig, String> {
    validateConfig(&config)?;
    let previous = loadConfig()?;
    persistConfig(&config)?;
    if let Err(error) = applyRuntime(&config) {
        let _ = persistConfig(&previous);
        let _ = applyRuntime(&previous);
        return Err(error);
    }
    Ok(config)
}

fn persistConfig(config: &ProxyConfig) -> Result<(), String> {
    let encoded =
        serde_json::to_string(config).map_err(|error| format!("序列化代理配置失败：{error}"))?;
    let storage = crate::storage_helpers::open_storage().ok_or("打开代理配置存储失败")?;
    storage
        .set_app_setting(
            proxyConfigSettingKey,
            &encoded,
            codexmanager_core::storage::now_ts(),
        )
        .map_err(|error| format!("保存代理配置失败：{error}"))
}

// 启用时启动或热重载 mihomo，并把网关固定到本地 mixed-port；关闭时销毁子进程并恢复原出口。
fn applyRuntime(config: &ProxyConfig) -> Result<(), String> {
    let mut runtime = runtime().lock().map_err(|_| "代理内核状态锁损坏")?;
    if !config.enabled {
        let _ = crate::gateway::set_upstream_proxy_url(None)?;
        runtime.take();
        return Ok(());
    }
    let restart = match runtime.as_mut() {
        Some(kernel) => !kernel.is_running()?,
        None => true,
    };
    if restart {
        *runtime = Some(Kernel::start(config)?);
    } else if let Some(kernel) = runtime.as_ref() {
        kernel.apply(config)?;
    }
    let port = runtime.as_ref().ok_or("代理内核未就绪")?.mixed_port();
    crate::gateway::set_upstream_proxy_url(Some(&format!("http://127.0.0.1:{port}")))?;
    Ok(())
}

// 服务初始化时恢复上次开关；失败保留持久化配置并让调用方获得精确诊断。
pub fn restore() -> Result<ProxyRuntimeStatus, String> {
    let Some(config) = loadStoredConfig()? else {
        return statusFor(&ProxyConfig::default());
    };
    validateConfig(&config)?;
    applyRuntime(&config)?;
    statusFor(&config)
}

// 服务退出时释放子进程，避免安装更新或下次启动遇到孤儿内核和旧端口。
pub fn shutdown() -> Result<(), String> {
    runtime().lock().map_err(|_| "代理内核状态锁损坏")?.take();
    Ok(())
}

pub fn status() -> Result<ProxyRuntimeStatus, String> {
    statusFor(&loadConfig()?)
}

fn statusFor(config: &ProxyConfig) -> Result<ProxyRuntimeStatus, String> {
    let runtime = runtime().lock().map_err(|_| "代理内核状态锁损坏")?;
    Ok(ProxyRuntimeStatus {
        running: runtime.is_some(),
        mixedPort: runtime.as_ref().map(Kernel::mixed_port),
        active: config.active.clone(),
        nodeCount: config.nodes.len(),
        groupCount: config.groups.len(),
        subscriptionCount: config.subscriptions.len(),
    })
}

// 切换节点走 mihomo REST API，不重启内核；完整选择状态仍由随后保存的配置负责持久化。
pub fn select(active: &str) -> Result<(), String> {
    runtime()
        .lock()
        .map_err(|_| "代理内核状态锁损坏")?
        .as_ref()
        .ok_or("代理内核未运行")?
        .select(active)
}

pub fn selectGroupMember(group: &str, member: &str) -> Result<(), String> {
    runtime()
        .lock()
        .map_err(|_| "代理内核状态锁损坏")?
        .as_ref()
        .ok_or("代理内核未运行")?
        .select_in(group, member)
}

pub fn testNodes() -> Result<Vec<NodeDelay>, String> {
    let config = loadConfig()?;
    if config.nodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut runtime = runtime().lock().map_err(|_| "代理内核状态锁损坏")?;
    let restart = match runtime.as_mut() {
        Some(kernel) => !kernel.is_running()?,
        None => true,
    };
    if restart {
        *runtime = Some(Kernel::start(&config)?);
    } else if let Some(kernel) = runtime.as_ref() {
        kernel.apply(&config)?;
    }
    Ok(runtime
        .as_ref()
        .ok_or("代理内核未就绪")?
        .test_nodes(&config.nodes))
}

// 单节点测速同样只经 mihomo；未启动时加载配置并懒启动内核，不经过系统代理客户端。
pub fn testNode(name: &str) -> Result<NodeDelay, String> {
    let config = loadConfig()?;
    if !config.nodes.iter().any(|node| node.name == name) {
        return Err(format!("代理节点不存在：{name}"));
    }
    let mut runtime = runtime().lock().map_err(|_| "代理内核状态锁损坏")?;
    let restart = match runtime.as_mut() {
        Some(kernel) => !kernel.is_running()?,
        None => true,
    };
    if restart {
        *runtime = Some(Kernel::start(&config)?);
    } else if let Some(kernel) = runtime.as_ref() {
        kernel.apply(&config)?;
    }
    Ok(runtime.as_ref().ok_or("代理内核未就绪")?.test_node(name))
}

// 出口查询必须经过当前内核 mixed-port；OpenAI trace 的 loc/colo 与画像探针使用同一证据来源。
pub fn testEgress() -> Result<EgressView, String> {
    let runtime = runtime().lock().map_err(|_| "代理内核状态锁损坏")?;
    let kernel = runtime.as_ref().ok_or("代理内核未运行")?;
    let proxy = format!("http://127.0.0.1:{}", kernel.mixed_port());
    let body = reqwest::blocking::Client::builder()
        .proxy(reqwest::Proxy::all(&proxy).map_err(|error| format!("创建内核代理失败：{error}"))?)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| format!("创建出口探针失败：{error}"))?
        .get("https://chatgpt.com/cdn-cgi/trace")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::text)
        .map_err(|error| format!("出口探针失败：{error}"))?;
    let field = |name: &str| {
        body.lines()
            .find_map(|line| {
                line.split_once('=')
                    .filter(|(key, _)| *key == name)
                    .map(|(_, value)| value.to_string())
            })
            .unwrap_or_default()
    };
    Ok(EgressView {
        via: kernel.mixed_port().to_string(),
        ip: field("ip"),
        country: field("loc"),
        city: field("colo"),
        isp: "OpenAI edge".to_string(),
        error: None,
    })
}

pub fn parseClashText(text: &str) -> Vec<ProxyNode> {
    kernel::parse_clash_subscription(text)
}

// 订阅请求使用成熟 HTTP 客户端并限制响应大小；解析仍复用复制进来的全协议 Clash 解析器。
pub fn fetchSubscription(url: &str) -> Result<Vec<ProxyNode>, String> {
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("创建订阅客户端失败：{error}"))?
        .get(url)
        .header("user-agent", "clash-verge/v2.0.2 mihomo")
        .send()
        .map_err(|error| format!("拉取订阅失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("订阅服务器返回 HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length as usize > maxSubscriptionBytes)
    {
        return Err("订阅响应过大".to_string());
    }
    let body = response
        .text()
        .map_err(|error| format!("读取订阅响应失败：{error}"))?;
    if body.len() > maxSubscriptionBytes {
        return Err("订阅响应过大".to_string());
    }
    Ok(parseClashText(&body))
}

#[cfg(test)]
#[path = "../../tests/embeddedProxy/runtimeTests.rs"]
mod tests;
