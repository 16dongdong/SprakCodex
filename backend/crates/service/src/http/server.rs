use crate::http::backend_runtime::{start_backend_server, wake_backend_shutdown};
use crate::http::proxy_runtime::run_front_proxy;

/// 函数 `start_http`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
pub fn start_http(addr: &str) -> std::io::Result<()> {
    let backend = start_backend_server()?;
    let result = run_front_proxy(addr, &backend.addr);
    // 观测运行时与服务同生共灭，服务停止时排空统计并移除公开证书，避免监听残留。
    if let Err(error) = crate::directObservation::shutdownRuntime() {
        log::error!("服务停止时清理直连观测失败：{error}");
    }
    wake_backend_shutdown(&backend.addr);
    let _ = backend.join.join();
    result
}
