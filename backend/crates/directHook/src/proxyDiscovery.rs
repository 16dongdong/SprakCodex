//! 只识别实际配置的本地明文 HTTP 代理端点；不改变代理选择，不读取 PAC 内容，不缓存含凭据的 URL。
use std::{
    collections::BTreeSet,
    net::{IpAddr, SocketAddr},
};
use url::{Host, Url};
use windows::{core::w, Win32::System::Environment::GetEnvironmentVariableW};

const maxEnvironmentChars: usize = 32768;

// localhost 允许系统解析为 IPv4 或 IPv6；字面地址只匹配该地址，不能仅按端口捕获其他本地服务。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProxyHost {
    Localhost,
    Address(IpAddr),
}

// 持久状态只保留非秘密端点和协议判断；同一端点出现不兼容配置时保持原连接，避免错误的明文接入。
#[derive(Default)]
pub(super) struct ProxyEndpoints {
    plain: BTreeSet<(ProxyHost, u16)>,
    incompatible: BTreeSet<(ProxyHost, u16)>,
}

// 只在本地新连接决策时读取，发送热路径不查询设置；每次重新读取以支持代理切换而不复用过期端口。
pub(super) fn isPlainHttpProxy(destination: SocketAddr) -> bool {
    if !destination.ip().is_loopback() {
        return false;
    }
    let mut endpoints = ProxyEndpoints::default();
    for variable in [w!("HTTP_PROXY"), w!("HTTPS_PROXY"), w!("ALL_PROXY")] {
        let mut value = vec![0u16; maxEnvironmentChars];
        let length = unsafe { GetEnvironmentVariableW(variable, Some(&mut value)) } as usize;
        if length == 0 || length >= value.len() {
            continue;
        }
        if let Ok(value) = String::from_utf16(&value[..length]) {
            endpoints.addUrl(&value, "http");
        }
    }
    super::systemProxy::appendCurrent(&mut endpoints);
    endpoints.matches(destination)
}

impl ProxyEndpoints {
    // 使用成熟 URL 解析器，用户信息仅用于排除不支持的认证边界，不保留或输出原字符串。
    fn addUrl(&mut self, value: &str, defaultScheme: &str) {
        let expanded;
        let address = if value.contains("://") {
            value
        } else {
            expanded = format!("{defaultScheme}://{value}");
            &expanded
        };
        let Ok(parsed) = Url::parse(address) else {
            return;
        };
        // URL 的非特殊协议可能把 IPv4 留为 Domain；再次按 Host 规则规范化，保证 SOCKS 冲突也能识别。
        let Some(host) = parsed.host_str().and_then(|host| Host::parse(host).ok()) else {
            return;
        };
        let host = match host {
            Host::Domain(host) if host.eq_ignore_ascii_case("localhost") => ProxyHost::Localhost,
            Host::Ipv4(address) if address.is_loopback() => ProxyHost::Address(address.into()),
            Host::Ipv6(address) if address.is_loopback() => ProxyHost::Address(address.into()),
            _ => return,
        };
        let Some(port) = parsed.port_or_known_default().filter(|port| *port != 0) else {
            return;
        };
        let compatible = parsed.scheme() == "http"
            && parsed.username().is_empty()
            && parsed.password().is_none();
        let destination = if compatible {
            &mut self.plain
        } else {
            &mut self.incompatible
        };
        destination.insert((host, port));
    }

    // Windows 的 https= 选择 HTTPS 请求所用的 HTTP 代理，不等价于 https:// 加密代理；显式协议保持独立。
    pub(super) fn addSystemList(&mut self, list: &str) {
        for segment in list
            .split(';')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
        {
            let fields: Vec<_> = segment.split_whitespace().collect();
            if fields.len() == 2 && !fields[0].contains('=') {
                match fields[0].to_ascii_lowercase().as_str() {
                    "proxy" | "http" => self.addUrl(fields[1], "http"),
                    "https" => self.addUrl(fields[1], "https"),
                    "socks" | "socks4" | "socks5" => self.addUrl(fields[1], "socks5"),
                    _ => {}
                }
                continue;
            }
            if fields.len() > 1 && !fields.iter().all(|field| field.contains('=')) {
                continue;
            }
            for field in fields {
                self.addSystemEntry(field);
            }
        }
    }

    // 只处理已知静态代理项；未知选择器和 DIRECT 不产生端点，也不靠分割后的碎片猜测地址。
    fn addSystemEntry(&mut self, entry: &str) {
        match entry.split_once('=') {
            Some((key, value)) => match key.to_ascii_lowercase().as_str() {
                "http" | "https" => self.addUrl(value, "http"),
                "socks" | "socks4" | "socks5" => self.addUrl(value, "socks5"),
                _ => {}
            },
            None if !entry.eq_ignore_ascii_case("DIRECT") => self.addUrl(entry, "http"),
            None => {}
        }
    }

    // 先排除同端点的认证或协议冲突；此处不选择代理，只有应用已经连接该端点时才可能返回 true。
    fn matches(&self, destination: SocketAddr) -> bool {
        // 有序集合不依赖 Rust 的线程随机种子，适合未注册静态 TLS 的内存映像；查询仍保持对数复杂度。
        let exact = (ProxyHost::Address(destination.ip()), destination.port());
        let localhost = (ProxyHost::Localhost, destination.port());
        let matches = |endpoints: &BTreeSet<_>| {
            endpoints.contains(&exact)
                || (destination.ip().is_loopback() && endpoints.contains(&localhost))
        };
        matches(&self.plain) && !matches(&self.incompatible)
    }
}

#[cfg(test)]
#[path = "../tests/unit/proxyDiscoveryTests.rs"]
mod tests;
