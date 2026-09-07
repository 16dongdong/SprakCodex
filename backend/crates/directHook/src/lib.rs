//! 进程内网络观测模块：仅将选中的 TCP 连接接入宿主 Relay，保持时区、语言、注册表及子进程生命周期不变。
//! 来源协议与回调 ABI 保留；不携带原工程的画像改写、代理隐藏或子进程终止功能。
#[cfg(windows)]
#[allow(non_snake_case, non_upper_case_globals)]
mod hookInstall;

#[cfg(windows)]
#[path = "windowsRuntime.rs"]
#[allow(non_snake_case)]
mod imp;
