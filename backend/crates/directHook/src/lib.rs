//! 进程内网络观测模块：将选中的 TCP 连接接入宿主 Relay，并把出口时区与区域画像同步给目标进程。
//! 来源协议与回调 ABI 保留；不修改登录状态、注册表或子进程生命周期。
#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod hookInstall;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod environmentIdentity;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod relayControl;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod trustBundle;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod trustProvider;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod proxyDiscovery;

#[cfg(windows)]
#[allow(non_snake_case)]
mod systemProxy;

#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod runtimeMetadata;

#[cfg(all(windows, target_arch = "x86_64"))]
#[allow(non_snake_case, non_upper_case_globals)]
mod nativeCompletion;

#[cfg(windows)]
#[path = "windowsRuntime.rs"]
#[allow(non_snake_case, non_upper_case_globals)]
mod imp;

#[cfg(windows)]
#[allow(non_snake_case)]
mod warmConnections;
