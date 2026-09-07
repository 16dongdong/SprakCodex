//! 传输速率使用单调时钟的原始精度；展示用整数毫秒不参与计算，避免短传输被误判为没有用量。
#![allow(non_snake_case)]
use std::time::Duration;

// 上传、下载及中途结束共用计算入口；非正字节或真正零耗时返回未知，不虚构最小时长或零速率。
pub(super) fn measuredMbps(bytes: i64, elapsed: Duration) -> Option<f64> {
    if bytes <= 0 || elapsed.is_zero() {
        return None;
    }
    Some(super::cloudflare_style::stats::calculate_mbps(
        bytes as u64,
        elapsed,
    ))
}

#[cfg(test)]
#[path = "../../../tests/proxy/throughputTests.rs"]
mod tests;
