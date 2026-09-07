use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

// 已取消的常驻任务不读取系统目录，更不尝试打开任何进程；路径不会被使用。
#[tokio::test]
async fn cancelledMonitorDoesNotEnumerate() {
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    run(PathBuf::from("unused.dll"), cancelled, || {
        panic!("停用后不应枚举进程")
    })
    .await;
}

// 第一次扫描立即执行；由选择器触发停止后任务按时退出，不遗留后台扫描循环。
#[tokio::test]
async fn firstScanIsImmediateAndShutdownIsPrompt() {
    let cancelled = CancellationToken::new();
    let stopAfterScan = cancelled.clone();
    let scans = Arc::new(AtomicUsize::new(0));
    let observedScans = scans.clone();
    tokio::time::timeout(
        Duration::from_secs(1),
        run(PathBuf::from("unused.dll"), cancelled, move || {
            observedScans.fetch_add(1, Ordering::Relaxed);
            stopAfterScan.cancel();
            Ok(Vec::new())
        }),
    )
    .await
    .expect("停止扫描必须及时返回");
    assert_eq!(scans.load(Ordering::Relaxed), 1);
}
