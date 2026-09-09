//! Relay 配置是宿主与 DLL 的共同契约；运行线程身份限制配置寿命，不包含登录或请求内容。
use serde::{Deserialize, Serialize};

// 第十一版为配置头加入提交序列；稳定标识隔离旧内存映像，防止其误读新版快照。
#[allow(non_upper_case_globals)]
pub const deploymentIdentity: &str = "embedded-observation-hook-11";

// 创建时间使用 Windows FILETIME 原始 100ns 单位，线程 ID 被复用时仍能区分运行实例。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub processId: u32,
    pub threadId: u32,
    pub createdAt: u64,
}

// 保留既有网络字段的 wire name；缺少运行实例的旧配置只允许停用，不再触发改连。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RelayConfig {
    #[serde(rename = "proxy_relay_port")]
    pub relayPort: u16,
    #[serde(rename = "force_proxy_tcp")]
    pub forceProxyTcp: bool,
    #[serde(rename = "runtime_owner")]
    pub owner: Option<RuntimeIdentity>,
    #[serde(rename = "ca_certificate_path")]
    pub caCertificatePath: Option<std::path::PathBuf>,
    #[serde(rename = "completion_enabled")]
    pub completionEnabled: bool,
    #[serde(rename = "completion_directory")]
    pub completionDirectory: Option<std::path::PathBuf>,
}
