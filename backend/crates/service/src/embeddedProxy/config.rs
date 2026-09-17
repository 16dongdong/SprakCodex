//! 上游代理配置(Clash 节点 + 链式 + 订阅)。
//!
//! 节点保留**原始 Clash 字段**(协议特有字段都在 [`ProxyNode::extra`]),这样 vmess/vless/ss/
//! hysteria2/tuic/wireguard/reality 等任意协议都能承载,直接喂给 mihomo 内核。宿主 自己只读
//! name/kind/server/port 用于 UI 展示与分组,真正的协议连接由内核负责。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn is_zero(n: &u16) -> bool {
    *n == 0
}

/// 一个代理节点:固定的展示字段 + 原始 Clash 协议字段(`extra`)+ 宿主 扩展(链式/归属订阅)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyNode {
    pub name: String,
    /// 协议类型(对应 Clash 的 `type`):trojan/vmess/vless/ss/hysteria2/tuic/wireguard/...
    #[serde(default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub port: u16,
    /// 链式入口节点名(宿主 扩展):喂内核时转成该节点的 `dialer-proxy`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_entry: Option<String>,
    /// 归属订阅名(宿主 扩展);手动添加的节点为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// 其余原始 Clash 字段(password/uuid/cipher/sni/skip-cert-verify/ws-opts/reality-opts/…)。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 一个代理组(Clash proxy-group):把若干节点聚成一个可当出口/入口的整体。
///
/// `kind=select` 手动指定组内成员(`selected`);`kind=url-test` 按延迟+可用性自动选最优。
/// 成员只放**节点名**(不放组名),从数据上杜绝组套组成环。喂内核时:出口可直接选组名
/// (GLOBAL 成员含组名),链式入口也可选组名(节点的 `dialer-proxy` 指向组)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyGroup {
    /// 组名(唯一;不得与节点名或 mihomo 保留名 DIRECT/REJECT/GLOBAL 等冲突)。
    pub name: String,
    /// 选路方式:`select`(手动)| `url-test`(自动按延迟)。
    #[serde(default)]
    pub kind: String,
    /// 组成员(节点名)。
    #[serde(default)]
    pub members: Vec<String>,
    /// select 组当前手选的成员;url-test 组留 None(内核自动选)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
}

/// 一个订阅源:从 URL 拉取并解析出节点列表(更新时按 `name` 替换该订阅旧节点)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// 订阅名(唯一;节点的 `sub` 字段按名归属)。
    pub name: String,
    /// 订阅 URL。
    pub url: String,
    /// 上次更新的 Unix 秒(0 = 从未更新)。
    #[serde(default)]
    pub updated_at: u64,
    /// 上次拉到的节点数。
    #[serde(default)]
    pub node_count: u32,
}

/// 上游转发配置:节点列表 + 当前节点 + 启用开关 + 订阅源(Clash 风格)。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// 启用代理开关:开=走选中节点;关=原始直连,不跟随系统代理。
    #[serde(default)]
    pub enabled: bool,
    /// 当前选中的节点名。
    #[serde(default)]
    pub active: Option<String>,
    /// 节点列表。
    #[serde(default)]
    pub nodes: Vec<ProxyNode>,
    /// 代理组列表(出口/链式入口可选组)。
    #[serde(default)]
    pub groups: Vec<ProxyGroup>,
    /// 是否对目标连接做 TLS MITM 解密抓包。默认关闭,由用户显式开启。
    #[serde(default)]
    pub mitm: bool,
    /// 订阅源列表。
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_sub_and_extra_survive_serde_round_trip() {
        let mut extra = Map::new();
        extra.insert("password".into(), Value::String("pw".into()));
        let node = ProxyNode {
            name: "测试节点".into(),
            kind: "trojan".into(),
            server: "e.com".into(),
            port: 443,
            chain_entry: None,
            sub: Some("测试订阅".into()),
            extra,
        };
        let cfg = ProxyConfig {
            nodes: vec![node],
            subscriptions: vec![Subscription {
                name: "测试订阅".into(),
                url: "http://e.com/sub".into(),
                updated_at: 0,
                node_count: 1,
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProxyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.nodes[0].sub.as_deref(),
            Some("测试订阅"),
            "sub 丢失!序列化输出 = {json}"
        );
        assert_eq!(
            back.nodes[0].extra.get("password").and_then(|v| v.as_str()),
            Some("pw")
        );
        assert_eq!(back.subscriptions.len(), 1);
    }

    #[test]
    fn default_mitm_is_off() {
        let cfg = ProxyConfig::default();

        assert!(!cfg.mitm);
    }
}
