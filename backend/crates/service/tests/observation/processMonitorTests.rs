use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};

// 调度测试只使用虚构身份和替身加载器，绝不把这些 PID 传入 Windows 原生加载器。
fn fixtureCandidate(pid: u32) -> ProcessCandidate {
    ProcessCandidate {
        pid,
        createdAt: pid as u64,
        executable: PathBuf::from("fixture.exe"),
    }
}

// 等待确定的状态条件而非固定睡眠；超时直接失败，避免以“未观察到”替代并发证据。
async fn waitUntil(predicate: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("调度状态等待超时");
}

// 已取消的常驻任务不读取系统目录，更不尝试打开任何进程；路径不会被使用。
#[tokio::test]
async fn cancelledMonitorDoesNotEnumerate() {
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    run(
        PathBuf::from("unused.dll"),
        cancelled,
        || panic!("停用后不应枚举进程"),
        |_, _| Ok(()),
    )
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
        run(
            PathBuf::from("unused.dll"),
            cancelled,
            move || {
                observedScans.fetch_add(1, Ordering::Relaxed);
                stopAfterScan.cancel();
                Ok(Vec::new())
            },
            |_, _| Ok(()),
        ),
    )
    .await
    .expect("停止扫描必须及时返回");
    assert_eq!(scans.load(Ordering::Relaxed), 1);
}

// 一个加载挂起后仍发现新实例；停用等待已开始事务，既不重复加载也不提前声明回收完成。
#[tokio::test]
async fn slowLoadDoesNotBlockDiscoveryAndStopDrains() {
    let cancel = CancellationToken::new();
    let started = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let (release, blocked) = mpsc::channel();
    let blocked = Mutex::new(blocked);
    let selectionStarted = started.clone();
    let loadingStarted = started.clone();
    let loadingCompleted = completed.clone();
    let monitor = tokio::spawn(runWithLoader(
        cancel.clone(),
        move || {
            let mut candidates = vec![fixtureCandidate(1)];
            if selectionStarted.load(Ordering::SeqCst) > 0 {
                candidates.push(fixtureCandidate(2));
            }
            Ok(candidates)
        },
        move |candidate| {
            loadingStarted.fetch_add(1, Ordering::SeqCst);
            if candidate.pid == 1 {
                blocked
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| "测试释放信号缺失")?;
            }
            loadingCompleted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    ));
    waitUntil(|| completed.load(Ordering::SeqCst) == 1).await;
    assert_eq!(started.load(Ordering::SeqCst), 2);
    cancel.cancel();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!monitor.is_finished(), "停止返回前必须等待未完成加载");
    release.send(()).unwrap();
    monitor.await.unwrap();
    assert_eq!(completed.load(Ordering::SeqCst), 2);
}

// 目录重复项和持续扫描均不重复派发；首批阻塞时并发不超过上限，释放后后续实例最终全部完成。
#[tokio::test]
async fn loadsAreBoundedDeduplicatedAndEventuallyComplete() {
    let cancel = CancellationToken::new();
    let started = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let scans = Arc::new(AtomicUsize::new(0));
    let (release, blocked) = mpsc::channel();
    let blocked = Mutex::new(blocked);
    let selectionScans = scans.clone();
    let loadingStarted = started.clone();
    let loadingCompleted = completed.clone();
    let total = maxConcurrentLoads * 2 + 1;
    let monitor = tokio::spawn(runWithLoader(
        cancel.clone(),
        move || {
            selectionScans.fetch_add(1, Ordering::SeqCst);
            Ok((1..=total as u32)
                .flat_map(|pid| [fixtureCandidate(pid), fixtureCandidate(pid)])
                .collect())
        },
        move |_| {
            let ordinal = loadingStarted.fetch_add(1, Ordering::SeqCst);
            if ordinal < maxConcurrentLoads {
                blocked
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| "测试释放信号缺失")?;
            }
            loadingCompleted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    ));
    waitUntil(|| scans.load(Ordering::SeqCst) >= 3).await;
    assert_eq!(started.load(Ordering::SeqCst), maxConcurrentLoads);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    for _ in 0..maxConcurrentLoads {
        release.send(()).unwrap();
    }
    waitUntil(|| completed.load(Ordering::SeqCst) == total).await;
    cancel.cancel();
    monitor.await.unwrap();
    assert_eq!(started.load(Ordering::SeqCst), total);
}

// 阻塞线程异常也必须解除 Loading；同 PID 的新创建时间属于另一实例，不受旧实例冷却影响。
#[tokio::test]
async fn panicRetriesAndPidReuseKeepsIndependentIdentity() {
    let cancel = CancellationToken::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let loadingAttempts = attempts.clone();
    let loadingCompleted = completed.clone();
    let retryStart = Arc::new(Mutex::new(None));
    let loadingRetry = retryStart.clone();
    let monitor = tokio::spawn(runWithLoader(
        cancel.clone(),
        || {
            let original = fixtureCandidate(1);
            let mut replacement = original.clone();
            replacement.createdAt += 1;
            Ok(vec![original, replacement])
        },
        move |candidate| {
            if candidate.createdAt == 1 {
                if loadingAttempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    *loadingRetry.lock().unwrap() = Some(Instant::now());
                    panic!("预期的测试加载异常");
                }
                assert!(loadingRetry.lock().unwrap().unwrap().elapsed() >= retryInterval);
            }
            loadingCompleted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    ));
    waitUntil(|| completed.load(Ordering::SeqCst) == 2).await;
    cancel.cancel();
    monitor.await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}
