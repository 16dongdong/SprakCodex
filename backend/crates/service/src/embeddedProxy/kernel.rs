//! mihomo(Clash.Meta)内核管理:启动/停止子进程、生成其配置、经 RESTful API 切节点/测延迟。
//!
//! 宿主 把目标进程劫持下来的流量经 mihomo 的本地 `mixed-port` 转发,由内核处理**全部协议**
//! (vmess/vless/ss/trojan/hysteria2/tuic/wireguard/reality/…)。宿主 自己只管 hook 劫持、
//! MITM 解密抓包、指纹改写;协议连接交给内核。节点切换走内核 API,relay 上游恒为本地 mixed 端口。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use super::config::{ProxyConfig, ProxyNode};

/// mihomo 里承载“当前出口”的选择器组名(mode=global 时全局走它)。
const GROUP: &str = "GLOBAL";
/// 内核 API 在本机 loopback 上,正常响应只有几 KB。这里仍设总量上限,避免异常内核或被本机进程误连
/// 时把 `/proxies` 等响应一次性读到无限增长的 String。
const MAX_KERNEL_API_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// 节点测速结果(供 UI 显示):延迟、错误、出口国家码。
#[derive(Debug, Clone, Serialize)]
pub struct NodeDelay {
    pub name: String,
    pub delay: Option<u32>,
    pub error: Option<String>,
    pub country_code: Option<String>,
}

/// 一个运行中的 mihomo 内核实例。
pub struct Kernel {
    mixed_port: u16,
    api_port: u16,
    secret: String,
    config_path: std::path::PathBuf,
    child: Child,
}

impl Kernel {
    /// relay 上游要连的本地 mixed(SOCKS5/HTTP)端口。
    pub fn mixed_port(&self) -> u16 {
        self.mixed_port
    }

    /// 启动内核:生成配置 + 拉起进程 + 等 API 就绪 + 选中当前节点。
    pub fn start(proxy: &ProxyConfig) -> Result<Kernel, String> {
        let exe_dir = crate::process_env::exe_dir();
        let mihomo = mihomo_path(&exe_dir);
        if !mihomo.exists() {
            return Err(format!("找不到内核 mihomo.exe:{}", mihomo.display()));
        }
        let data_dir = exe_dir.join("data").join("mihomo");
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建内核目录失败:{e}"))?;
        let mixed_port = free_port()?;
        let api_port = free_port()?;
        let secret = gen_secret();
        let config_path = data_dir.join("config.yaml");
        write_config(&config_path, proxy, mixed_port, api_port, &secret)?;

        let mut cmd = Command::new(&mihomo);
        cmd.arg("-d")
            .arg(&data_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd.spawn().map_err(|e| format!("启动 mihomo 失败:{e}"))?;

        let kernel = Kernel {
            mixed_port,
            api_port,
            secret,
            config_path,
            child,
        };
        kernel.wait_ready()?;
        kernel.apply_selections(proxy);
        Ok(kernel)
    }

    fn wait_ready(&self) -> Result<(), String> {
        for _ in 0..60 {
            if self.api_get("/version").is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("mihomo API 未就绪(启动超时)".to_string())
    }

    /// 节点/订阅变化后:重写配置并热重载,再重选当前节点。
    pub fn apply(&self, proxy: &ProxyConfig) -> Result<(), String> {
        write_config(
            &self.config_path,
            proxy,
            self.mixed_port,
            self.api_port,
            &self.secret,
        )?;
        let body = json!({ "path": self.config_path.to_string_lossy() }).to_string();
        self.api_request("PUT", "/configs?force=true", Some(&body))?;
        self.apply_selections(proxy);
        Ok(())
    }

    /// 在某个 select 组里切换当前选中:`PUT /proxies/{group} {name:target}`。
    pub fn select_in(&self, group: &str, target: &str) -> Result<(), String> {
        let body = json!({ "name": target }).to_string();
        self.api_request(
            "PUT",
            &format!("/proxies/{}", urlencode(group)),
            Some(&body),
        )?;
        Ok(())
    }

    /// 切换全局出口(GLOBAL selector;mode=global 下全局生效)。出口可为节点名或组名。
    pub fn select(&self, target: &str) -> Result<(), String> {
        self.select_in(GROUP, target)
    }

    /// 重放内核里所有「手动选择」:先各 select 子组的 `selected`,最后 GLOBAL 的 `active`。
    /// 顺序关键——GLOBAL 指向某组时要先选好子组成员,再定 GLOBAL 指针;url-test 组自愈不用管。
    /// 热重载(force=true)会把 selector 重置到首个成员,故每次 start/apply 都要重放本函数。
    fn apply_selections(&self, proxy: &ProxyConfig) {
        for g in &proxy.groups {
            if g.kind == "select" {
                if let Some(sel) = g.selected.as_deref().filter(|s| !s.is_empty()) {
                    if g.members.iter().any(|m| m == sel) {
                        let _ = self.select_in(&g.name, sel);
                    }
                }
            }
        }
        if let Some(active) = proxy.active.as_deref().filter(|s| !s.is_empty()) {
            let _ = self.select(active);
        }
    }

    /// 经内核**一次性并发**测整个 GLOBAL 组所有节点延迟:mihomo 内部并发探测,
    /// 一次 API 调用拿回 `{节点名: 延迟ms}`,总耗时≈单次超时而非 N×超时(逐个串行会卡死 UI)。
    /// 出口国家码暂留 None(UI 按节点名兜底显示国旗)。
    pub fn test_nodes(&self, nodes: &[ProxyNode]) -> Vec<NodeDelay> {
        let path = format!(
            "/group/{}/delay?url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204&timeout=5000",
            urlencode(GROUP)
        );
        // 返回 { "节点名": 延迟ms, ... };测不通的节点不在 map 内。
        let map = self
            .api_get(&path)
            .ok()
            .and_then(|resp| {
                serde_json::from_str::<serde_json::Map<String, Value>>(body_of(&resp)?).ok()
            })
            .unwrap_or_default();
        nodes
            .iter()
            .map(|n| {
                let delay = map.get(&n.name).and_then(|v| v.as_u64()).map(|x| x as u32);
                NodeDelay {
                    name: n.name.clone(),
                    error: if delay.is_none() {
                        Some("超时或不可用".to_string())
                    } else {
                        None
                    },
                    delay,
                    country_code: None,
                }
            })
            .collect()
    }

    fn api_get(&self, path: &str) -> Result<String, String> {
        self.api_request("GET", path, None)
    }

    fn api_request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.api_port))
            .map_err(|e| format!("连内核 API 失败:{e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
        let body = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.secret,
            body.len(),
            body
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("发内核 API 请求失败:{e}"))?;
        let resp = read_kernel_api_response(&mut stream)?;
        let status = resp
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse::<u16>().ok())
            .unwrap_or(0);
        if (200..300).contains(&status) {
            Ok(resp)
        } else {
            Err(format!("内核 API {path} 返回 {status}"))
        }
    }
}

fn read_kernel_api_response<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut resp = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if resp.len() + n > MAX_KERNEL_API_RESPONSE_BYTES {
                    return Err("内核 API 响应过大".to_string());
                }
                resp.extend_from_slice(&chunk[..n]);
            }
            Err(e) => return Err(format!("读内核 API 响应失败:{e}")),
        }
    }
    String::from_utf8(resp).map_err(|_| "内核 API 响应不是 UTF-8".to_string())
}

impl Drop for Kernel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// 发布版只接受安装目录资源；调试测试允许读取仓库内固定第三方资产，不依赖外部工程路径。
fn mihomo_path(exe_dir: &std::path::Path) -> std::path::PathBuf {
    let installed = exe_dir.join("mihomo.exe");
    if installed.exists() {
        return installed;
    }
    #[cfg(debug_assertions)]
    {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../third_party/mihomo/mihomo.exe");
        if source.exists() {
            return source;
        }
    }
    installed
}

fn body_of(resp: &str) -> Option<&str> {
    resp.split_once("\r\n\r\n").map(|(_, b)| b)
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn gen_secret() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("proxy{nanos:x}")
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 节点 → Clash node:`kind`→`type`、链式 `chain_entry`→`dialer-proxy`,并兼容历史字段名 `skip_verify`。
fn node_to_clash(node: &ProxyNode) -> Value {
    let mut m = node.extra.clone();
    m.insert("name".into(), Value::String(node.name.clone()));
    m.insert("type".into(), Value::String(node.kind.clone()));
    if !node.server.is_empty() {
        m.insert("server".into(), Value::String(node.server.clone()));
    }
    if node.port != 0 {
        m.insert("port".into(), Value::Number(node.port.into()));
    }
    if let Some(v) = m.remove("skip_verify") {
        m.entry("skip-cert-verify".to_string()).or_insert(v);
    }
    if let Some(entry) = node.chain_entry.as_deref().filter(|s| !s.is_empty()) {
        m.insert("dialer-proxy".into(), Value::String(entry.to_string()));
    }
    Value::Object(m)
}

/// mihomo 内置/保留名:用户组名不能与之冲突,否则整份配置被拒。
const RESERVED_NAMES: [&str; 5] = ["DIRECT", "REJECT", "GLOBAL", "PASS", "COMPATIBLE"];
/// url-test 组自动测速地址。
const GROUP_TEST_URL: &str = "http://www.gstatic.com/generate_204";

/// 由用户组构建 mihomo `proxy-groups`(只发**幸存**组)+ 返回幸存组名列表(供 GLOBAL 组装)。
///
/// 必须做完整过滤:任何一个非法组都会让 mihomo **拒绝整份配置**(→ 全部路由失效)。剔除规则:
/// 组名空/与节点同名/保留名/与更早的组重名 → 丢整组;成员过滤到存在的节点、去重、丢弃
/// `chain_entry==本组` 的成员(防环);成员清空后丢整组。
fn build_proxy_groups(proxy: &ProxyConfig) -> (Vec<Value>, Vec<String>) {
    use std::collections::HashSet;
    let node_names: HashSet<&str> = proxy.nodes.iter().map(|n| n.name.as_str()).collect();

    let mut emitted: Vec<Value> = Vec::new();
    let mut surviving: Vec<String> = Vec::new();
    let mut seen_group: HashSet<String> = HashSet::new();

    for g in &proxy.groups {
        let name = g.name.trim();
        if name.is_empty()
            || node_names.contains(name)
            || RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(name))
            || !seen_group.insert(name.to_string())
        {
            continue;
        }
        let mut members: Vec<String> = Vec::new();
        let mut seen_member: HashSet<&str> = HashSet::new();
        for m in &g.members {
            let m = m.as_str();
            if !node_names.contains(m) || !seen_member.insert(m) {
                continue;
            }
            // 防环:成员节点若以本组为链式入口(dialer-proxy=本组),不能同时是本组成员。
            let dial_through_self = proxy
                .nodes
                .iter()
                .find(|n| n.name == m)
                .and_then(|n| n.chain_entry.as_deref())
                .is_some_and(|e| e == name);
            if dial_through_self {
                continue;
            }
            members.push(m.to_string());
        }
        if members.is_empty() {
            continue;
        }
        let member_vals: Vec<Value> = members.into_iter().map(Value::String).collect();
        let obj = if g.kind == "url-test" {
            json!({
                "name": name, "type": "url-test", "proxies": member_vals,
                "url": GROUP_TEST_URL, "interval": 300, "tolerance": 50, "timeout": 5000,
            })
        } else {
            json!({ "name": name, "type": "select", "proxies": member_vals })
        };
        emitted.push(obj);
        surviving.push(name.to_string());
    }
    (emitted, surviving)
}

fn write_config(
    path: &std::path::Path,
    proxy: &ProxyConfig,
    mixed_port: u16,
    api_port: u16,
    secret: &str,
) -> Result<(), String> {
    use std::collections::HashSet;
    let proxies: Vec<Value> = proxy.nodes.iter().map(node_to_clash).collect();
    let (user_groups, surviving_groups) = build_proxy_groups(proxy);

    // GLOBAL 成员 = 节点名 + 幸存组名 + DIRECT(去重)。先确定幸存组,绝不引用被丢弃的组,
    // 否则 GLOBAL 引用不存在的 proxy → 整份配置被拒。
    let mut seen: HashSet<String> = HashSet::new();
    let mut global_members: Vec<Value> = Vec::new();
    for n in &proxy.nodes {
        if seen.insert(n.name.clone()) {
            global_members.push(Value::String(n.name.clone()));
        }
    }
    for gname in &surviving_groups {
        if seen.insert(gname.clone()) {
            global_members.push(Value::String(gname.clone()));
        }
    }
    global_members.push(Value::String("DIRECT".into()));

    let mut proxy_groups: Vec<Value> = user_groups;
    proxy_groups.push(json!({ "name": GROUP, "type": "select", "proxies": global_members }));

    let config = json!({
        "mixed-port": mixed_port,
        "bind-address": "127.0.0.1",
        "allow-lan": false,
        "mode": "global",
        "log-level": "silent",
        "external-controller": format!("127.0.0.1:{api_port}"),
        "secret": secret,
        "unified-delay": true,
        // 让 selector 选择跨热重载/重启保留(在 data/mihomo 写 cache.db);apply_selections 仍是权威来源。
        "profile": { "store-selected": true },
        "proxies": proxies,
        "proxy-groups": proxy_groups,
    });
    // mihomo 用 YAML 解析配置;serde_yaml 能把 JSON Value 直接序列化成合法 YAML。
    let yaml = serde_yaml::to_string(&config).map_err(|e| format!("生成内核配置失败:{e}"))?;
    std::fs::write(path, yaml).map_err(|e| format!("写内核配置失败:{e}"))
}

/// 解析 Clash 节点文本 → 节点(保留全部协议字段)。支持:`proxies:` 列表、整体 base64、
/// 顶层裸节点序列;以及缩进不规整 / 缺 `proxies:` 包裹的「粘贴」(严格 YAML 失败时退化为逐行解析)。
pub fn parse_clash_subscription(text: &str) -> Vec<ProxyNode> {
    let decoded = maybe_base64(text);
    // 1) 严格 YAML:`proxies:` 列表,或顶层本身就是节点序列。
    if let Ok(doc) = serde_yaml::from_str::<Value>(&decoded) {
        if let Some(arr) = doc
            .get("proxies")
            .and_then(Value::as_array)
            .or_else(|| doc.as_array())
        {
            return arr
                .iter()
                .filter_map(clash_to_node)
                .filter(|n| !is_info_node(n))
                .collect();
        }
    }
    // 2) 兜底:逐行宽松解析。专治「从 Clash 配置里抠出一段节点直接粘贴」——缩进不规整、
    //    缺 `proxies:` 包裹都能解析(对缩进完全不敏感,逐行 trim 后按 `- `/`key: value` 归并)。
    parse_clash_lenient(&decoded)
}

fn parse_clash_lenient(text: &str) -> Vec<ProxyNode> {
    fn flush(cur: &mut Option<serde_json::Map<String, Value>>, nodes: &mut Vec<ProxyNode>) {
        if let Some(node) = cur.take().and_then(map_to_node) {
            nodes.push(node);
        }
    }
    let mut nodes = Vec::new();
    let mut cur: Option<serde_json::Map<String, Value>> = None;
    for raw in text.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line == "proxies:" {
            continue;
        }
        if line == "-" {
            flush(&mut cur, &mut nodes);
            cur = Some(serde_json::Map::new());
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            flush(&mut cur, &mut nodes);
            cur = Some(serde_json::Map::new());
            line = rest.trim();
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            if !key.is_empty() {
                cur.get_or_insert_with(serde_json::Map::new)
                    .insert(key.to_string(), lenient_value(v.trim()));
            }
        }
    }
    flush(&mut cur, &mut nodes);
    nodes.into_iter().filter(|n| !is_info_node(n)).collect()
}

/// 解析一个标量值:**带引号一律按字符串**(避免 `password: '357159'` 被误判成数字),
/// 不带引号才推断 true/false/数字,其余按字符串。
fn lenient_value(raw: &str) -> Value {
    let b = raw.as_bytes();
    if b.len() >= 2 {
        let (f, l) = (b[0], b[b.len() - 1]);
        if (f == b'\'' && l == b'\'') || (f == b'"' && l == b'"') {
            return Value::String(raw[1..raw.len() - 1].to_string());
        }
    }
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => raw
            .parse::<u64>()
            .map(|n| Value::Number(n.into()))
            .unwrap_or_else(|_| Value::String(raw.to_string())),
    }
}

/// 字段映射 → 节点(无 server 视为无效条目过滤掉;无 name 用 kind-server 兜底)。
fn map_to_node(mut m: serde_json::Map<String, Value>) -> Option<ProxyNode> {
    let server = m
        .remove("server")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    if server.is_empty() {
        return None;
    }
    let kind = m
        .remove("type")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let port = m
        .remove("port")
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0) as u16;
    let name = m
        .remove("name")
        .and_then(|v| v.as_str().map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}-{}",
                if kind.is_empty() {
                    "node"
                } else {
                    kind.as_str()
                },
                server
            )
        });
    Some(ProxyNode {
        name,
        kind,
        server,
        port,
        chain_entry: None,
        sub: None,
        extra: m,
    })
}

/// 机场常把“剩余流量/到期/主站”等信息塞成指向本地回环的假节点,这些不是真实出口,过滤掉。
fn is_info_node(n: &ProxyNode) -> bool {
    matches!(
        n.server.as_str(),
        "" | "127.0.0.1" | "::1" | "localhost" | "0.0.0.0"
    )
}

fn clash_to_node(v: &Value) -> Option<ProxyNode> {
    let Value::Object(obj) = v else {
        return None;
    };
    let mut m = obj.clone();
    let name = m.remove("name")?.as_str()?.to_string();
    let kind = m
        .remove("type")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let server = m
        .remove("server")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let port = m.remove("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    Some(ProxyNode {
        name,
        kind,
        server,
        port,
        chain_entry: None,
        sub: None,
        extra: m,
    })
}

fn maybe_base64(text: &str) -> String {
    let t = text.trim();
    if t.starts_with("proxies:") || t.contains("\nproxies:") {
        return text.to_string();
    }
    if let Some(decoded) = base64_decode(t) {
        if let Ok(s) = String::from_utf8(decoded) {
            return s;
        }
    }
    text.to_string()
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let clean: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .map(|b| match b {
            b'-' => b'+',
            b'_' => b'/',
            x => x,
        })
        .collect();
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in &clean {
        let val = T.iter().position(|&t| t == c)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::super::config::{ProxyGroup, ProxyNode};
    use super::*;

    #[test]
    fn kernel_api_response_has_hard_limit() {
        let oversized = vec![b'a'; MAX_KERNEL_API_RESPONSE_BYTES + 1];
        let err = read_kernel_api_response(&mut std::io::Cursor::new(oversized))
            .expect_err("oversized response");

        assert_eq!(err, "内核 API 响应过大");
    }

    #[test]
    fn parses_pasted_nodes_with_irregular_indent() {
        // 用户原样粘贴:首个 `- type` 顶格、后续 key 缩进 4 格、第二个 `- ` 缩进 2 格(缩进不规整)。
        let pasted = "- type: 'trojan'\n    name: '🇺🇸 示例一'\n    server: 'us1.example.net'\n    port: 443\n    password: '123456'\n    sni: 'us1.example.net'\n    skip-cert-verify: true\n    udp: true\n  - type: 'trojan'\n    name: '🇯🇵 示例二'\n    server: 'jp1.example.net'\n    port: 443\n    password: '123456'\n    sni: 'jp1.example.net'\n    skip-cert-verify: true\n    udp: true\n  - type: 'socks5'\n    name: '🇺🇸 示例 SOCKS'\n    server: '198.51.100.10'\n    port: 45001\n    username: 'demoUser'\n    password: 'demoPassword'\n";
        let nodes = parse_clash_subscription(pasted);
        assert_eq!(nodes.len(), 3, "应解析出 3 个节点");
        assert_eq!(nodes[0].name, "🇺🇸 示例一");
        assert_eq!(nodes[0].kind, "trojan");
        assert_eq!(nodes[0].server, "us1.example.net");
        assert_eq!(nodes[0].port, 443);
        // 带引号的密码必须仍是字符串,不能被推断成数字
        assert_eq!(
            nodes[0].extra.get("password").and_then(|v| v.as_str()),
            Some("123456")
        );
        assert_eq!(
            nodes[0]
                .extra
                .get("skip-cert-verify")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(nodes[2].kind, "socks5");
        assert_eq!(nodes[2].port, 45001);
        assert_eq!(
            nodes[2].extra.get("username").and_then(|v| v.as_str()),
            Some("demoUser")
        );
        assert_eq!(
            nodes[2].extra.get("password").and_then(|v| v.as_str()),
            Some("demoPassword")
        );
    }

    #[test]
    fn strict_proxies_yaml_still_works() {
        let yaml = "proxies:\n  - name: A\n    type: trojan\n    server: a.com\n    port: 443\n    password: pw\n";
        let nodes = parse_clash_subscription(yaml);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "A");
        assert_eq!(nodes[0].server, "a.com");
        assert_eq!(nodes[0].port, 443);
    }

    fn node(name: &str) -> ProxyNode {
        ProxyNode {
            name: name.into(),
            kind: "trojan".into(),
            server: "x".into(),
            port: 1,
            chain_entry: None,
            sub: None,
            extra: serde_json::Map::new(),
        }
    }

    fn group(name: &str, kind: &str, members: &[&str]) -> ProxyGroup {
        ProxyGroup {
            name: name.into(),
            kind: kind.into(),
            members: members.iter().map(|s| s.to_string()).collect(),
            selected: None,
        }
    }

    fn members_of(g: &Value) -> Vec<&str> {
        g["proxies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    }

    #[test]
    fn filters_invalid_groups_and_dedups_members() {
        let cfg = ProxyConfig {
            nodes: vec![node("A"), node("B"), node("C")],
            groups: vec![
                group("ok-select", "select", &["A", "B", "B", "Z"]), // 去重 B、丢失踪 Z
                group("ok-url", "url-test", &["C"]),                 // 单成员合法
                group("empty", "select", &[]),                       // 丢:空
                group("all-bad", "select", &["Z", "Y"]),             // 丢:无合法成员
                group("GLOBAL", "select", &["A"]),                   // 丢:保留名
                group("A", "select", &["B"]),                        // 丢:与节点同名
                group("ok-select", "select", &["C"]),                // 丢:重名
            ],
            ..Default::default()
        };
        let (emitted, surviving) = build_proxy_groups(&cfg);
        assert_eq!(
            surviving,
            vec!["ok-select".to_string(), "ok-url".to_string()]
        );
        let sel = emitted.iter().find(|g| g["name"] == "ok-select").unwrap();
        assert_eq!(members_of(sel), vec!["A", "B"]);
        let url = emitted.iter().find(|g| g["name"] == "ok-url").unwrap();
        assert_eq!(url["type"], "url-test");
        assert_eq!(url["interval"], 300);
    }

    #[test]
    fn drops_cycle_member_dialing_through_own_group() {
        let mut a = node("A");
        a.chain_entry = Some("G".into()); // A 以 G 为链式入口
        let cfg = ProxyConfig {
            nodes: vec![a, node("B")],
            groups: vec![group("G", "url-test", &["A", "B"])], // A 不能同时是 G 的成员
            ..Default::default()
        };
        let (emitted, surviving) = build_proxy_groups(&cfg);
        assert_eq!(surviving, vec!["G".to_string()]);
        assert_eq!(members_of(&emitted[0]), vec!["B"]);
    }

    // 固定第三方资源必须能真实启动、发布 mixed-port 和响应控制 API，避免只验证 YAML 而漏掉打包内核不兼容。
    #[test]
    fn bundled_kernel_starts_and_exposes_control_api() {
        let config = ProxyConfig {
            enabled: true,
            active: Some("fixture-http".to_string()),
            nodes: vec![ProxyNode {
                name: "fixture-http".to_string(),
                kind: "http".to_string(),
                server: "127.0.0.1".to_string(),
                port: 9,
                chain_entry: None,
                sub: None,
                extra: serde_json::Map::new(),
            }],
            ..Default::default()
        };
        let kernel = Kernel::start(&config).expect("内置 mihomo 应成功启动");
        assert!(kernel.mixed_port() > 0);
        assert!(kernel.api_get("/version").unwrap().contains("200"));
    }
}
