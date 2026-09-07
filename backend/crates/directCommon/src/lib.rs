//! 宿主与进程内观测模块共用的 Relay 字节协议和就绪事件契约；静态链接，不启动外部程序。

pub mod hook_proxy;
pub mod hook_ready;

#[allow(non_snake_case)]
pub mod relayContract;

#[cfg(windows)]
#[allow(non_snake_case)]
pub mod runtimeLease;
