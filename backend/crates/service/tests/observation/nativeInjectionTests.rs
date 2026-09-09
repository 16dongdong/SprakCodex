use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

const fixtureReadyMarker: &str = "observation-fixture-ready";

// 每例启动一个当前测试程序的独立进程，只运行阻塞夹具，不接触用户正在使用的 Codex 进程。
struct Target {
    child: Child,
}
impl Target {
    // 子进程通过环境控制测试 DLL 的就绪与延迟；路径身份从实际进程读取。
    fn start(ready: bool, delayMs: u32) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "directObservation::nativeInjection::tests::fixtureTarget",
                "--ignored",
                "--nocapture",
            ])
            .env("OBSERVATION_FIXTURE_TARGET", "true")
            .env(
                "OBSERVATION_FIXTURE_SIGNAL_READY",
                if ready { "true" } else { "false" },
            )
            .env("OBSERVATION_FIXTURE_LOAD_DELAY_MS", delayMs.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut target = Self { child };
        let output = target.child.stdout.take().expect("夹具输出管道缺失");
        assert!(
            BufReader::new(output)
                .lines()
                .any(|line| line.expect("读取夹具输出失败").contains(fixtureReadyMarker)),
            "夹具未完成启动"
        );
        target
    }

    // 当前身份来自有效进程对象，不使用上轮保存的 PID 或模块地址。
    fn identity(&self) -> ProcessCandidate {
        candidate(self.child.id()).unwrap()
    }
}
impl Drop for Target {
    // 即使断言失败也终止本测试创建的子进程并 wait，防止遗留夹具进程。
    fn drop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                self.child.kill().expect("终止夹具失败");
                self.child.wait().expect("回收夹具失败");
            }
            Err(_) => panic!("读取夹具状态失败"),
        }
    }
}

// 测试 DLL 由 Cargo example 构建，普通测试不推断 release 文件，也不加载生产 hook。
fn fixtureDll() -> PathBuf {
    PathBuf::from(
        std::env::var_os("OBSERVATION_TEST_READY_DLL")
            .expect("先构建并指定 observationReadyFixture.dll"),
    )
}

// 原生测试显式读取本轮构建产物并泄漏到测试进程结束，匹配生产常驻静态载荷生命周期。
fn fixtureImage() -> &'static [u8] {
    Box::leak(std::fs::read(fixtureDll()).unwrap().into_boxed_slice())
}

// 子进程只保持存活；标记必须由 Target::start 提供，防止直接误运行时长时间等待。
#[test]
#[ignore = "由原生加载测试在独立子进程调用"]
fn fixtureTarget() {
    assert_eq!(
        std::env::var("OBSERVATION_FIXTURE_TARGET").as_deref(),
        Ok("true")
    );
    println!("{fixtureReadyMarker}");
    std::thread::sleep(Duration::from_secs(60));
}

// 就绪事件必须来自已加载的 DLL；重复调用复用模块，不再次执行加载或增加引用计数。
#[test]
#[ignore = "需要 OBSERVATION_TEST_READY_DLL"]
fn readyModuleAndRepeatedLoad() {
    let target = Target::start(true, 0);
    let identity = target.identity();
    let image = fixtureImage();
    inject(&identity, image).unwrap();
    let started = Instant::now();
    inject(&identity, image).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(target.child.id() > 0);
}

// 两个独立真实进程共用生产调度器；第一个故意延迟 DLL 加载，第二个必须先就绪，停止后无加载预留遗留。
#[test]
#[ignore = "需要 OBSERVATION_TEST_READY_DLL，运行约三秒"]
fn concurrentMonitorDoesNotSerializeNativeLoads() {
    let slow = Target::start(true, 3000);
    let fast = Target::start(true, 0);
    let candidates = vec![slow.identity(), fast.identity()];
    let selected = candidates.clone();
    let completions = std::sync::Arc::new(Mutex::new(Vec::new()));
    let observed = completions.clone();
    let cancel = tokio_util::sync::CancellationToken::new();
    let stop = cancel.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(20),
            super::super::processMonitor::run(
                fixtureImage(),
                cancel,
                move || Ok(selected.clone()),
                move |candidate| {
                    let mut completed = observed.lock().unwrap();
                    completed.push(candidate.pid);
                    if completed.len() == 2 {
                        stop.cancel();
                    }
                    Ok(())
                },
            ),
        )
        .await
        .expect("并发原生加载验收超时")
    });
    assert_eq!(
        *completions.lock().unwrap(),
        vec![fast.child.id(), slow.child.id()]
    );
    let reserved = pendingLoads.lock().unwrap();
    assert!(candidates.iter().all(|candidate| !reserved
        .as_ref()
        .unwrap()
        .contains(&(candidate.pid, candidate.createdAt))));
    println!("真实进程并发加载通过：快目标先就绪，慢目标随后完成，预留已释放");
}

// 已加载但没有就绪事件的模块不能被算为接管成功，也不通过重试反复增加 LoadLibrary 引用。
#[test]
#[ignore = "需要 OBSERVATION_TEST_READY_DLL"]
fn loadedModuleWithoutReadyIsFailure() {
    let target = Target::start(false, 0);
    let failure = inject(&target.identity(), fixtureImage()).unwrap_err();
    assert_eq!(failure, "观测内存模块已加载但未就绪");
}

// PID 相同但创建时间不同，必须在远程分配或加载前拒绝。
#[test]
#[ignore = "需要 OBSERVATION_TEST_READY_DLL"]
fn staleIdentityIsRejectedBeforeLoad() {
    let target = Target::start(true, 0);
    let mut identity = target.identity();
    identity.createdAt += 1;
    assert_eq!(
        inject(&identity, fixtureImage()).unwrap_err(),
        "目标进程实例已变化，请重新扫描"
    );
}

// 超时任务保留参数和预留直到远程线程结束；结束后再次检查应就绪且没有活跃加载遗留。
#[test]
#[ignore = "需要 OBSERVATION_TEST_READY_DLL，运行约十三秒"]
fn timedOutLoadIsNotDuplicatedAndEventuallyReaped() {
    let target = Target::start(true, loadTimeoutMs + 1000);
    let identity = target.identity();
    let image = fixtureImage();
    assert_eq!(
        inject(&identity, image).unwrap_err(),
        "观测内存部署超时，仍在等待安全回收"
    );
    assert_eq!(
        inject(&identity, image).unwrap_err(),
        "目标部署尚未完成或等待队列已满"
    );
    std::thread::sleep(Duration::from_secs(3));
    inject(&identity, image).unwrap();
    let guard = pendingLoads.lock().unwrap();
    assert!(!guard
        .as_ref()
        .unwrap()
        .contains(&(identity.pid, identity.createdAt)));
}

// 非注入单元测试覆盖 Windows 路径协议，扩展路径前缀不应造成重复模块判定失败。
#[test]
fn equivalentWindowsPathsHaveSameIdentity() {
    assert_eq!(
        normalizedPath(Path::new(r"\\?\D:\Fixture\Hook.dll")),
        normalizedPath(Path::new("d:/fixture/hook.dll"))
    );
}
