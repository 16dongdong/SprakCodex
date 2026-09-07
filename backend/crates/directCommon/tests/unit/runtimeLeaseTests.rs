use super::*;
use std::{
    io::{BufRead, Read, Write},
    process::{Child, Command, Stdio},
};

// 线程返回后内核对象保持可查询但已终止；验证不依赖配置写入、轮询间隔或 PID 再次枚举。
#[test]
fn threadExitInvalidatesLease() {
    let (identitySender, identityReceiver) = std::sync::mpsc::channel();
    let (exitSender, exitReceiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        identitySender.send(currentIdentity().unwrap()).unwrap();
        exitReceiver.recv().unwrap();
    });
    let identity = identityReceiver.recv().unwrap();
    let lease = RuntimeLease::open(identity).unwrap();
    assert!(lease.isActive());
    exitSender.send(()).unwrap();
    worker.join().unwrap();
    assert!(!lease.isActive());
    assert!(RuntimeLease::open(identity).is_err());
}

// 身份的每个维度都参与比较；创建时间和进程错误不得只靠仍存活的线程 ID 通过。
#[test]
fn changedIdentityIsRejected() {
    let identity = currentIdentity().unwrap();
    assert!(RuntimeLease::open(identity).unwrap().isActive());
    assert!(RuntimeLease::open(RuntimeIdentity {
        createdAt: identity.createdAt ^ 1,
        ..identity
    })
    .is_err());
    assert!(RuntimeLease::open(RuntimeIdentity {
        processId: 0,
        ..identity
    })
    .is_err());
    assert!(RuntimeLease::open(RuntimeIdentity {
        threadId: 0,
        ..identity
    })
    .is_err());
}

// 独立子进程模拟宿主硬退出，父测试只打开查询句柄；析构始终回收本次创建的进程。
struct OwnerProcess(Child);

impl Drop for OwnerProcess {
    // 仅处理当前测试创建的子进程；失败让测试报错，不静默遗留等待 stdin 的进程。
    fn drop(&mut self) {
        if self.0.try_wait().expect("查询宿主夹具状态").is_none() {
            self.0.kill().expect("终止宿主夹具");
        }
        self.0.wait().expect("回收宿主夹具");
    }
}

// 强制退出不执行配置清理；已有线程句柄仍须在退出后立即报告失效。
#[test]
fn hostCrashInvalidatesLeaseWithoutCleanup() {
    let mut child = OwnerProcess(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtimeLease::tests::runtimeOwnerFixture",
                "--ignored",
                "--nocapture",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动宿主夹具"),
    );
    let reader = std::io::BufReader::new(child.0.stdout.take().unwrap());
    let identity = reader
        .lines()
        .find_map(|line| {
            let line = line.expect("读取夹具身份");
            line.strip_prefix("LEASE ")
                .map(|encoded| serde_json::from_str(encoded).unwrap())
        })
        .expect("夹具必须发布运行身份");
    let lease = RuntimeLease::open(identity).unwrap();
    assert!(lease.isActive());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(!lease.isActive());
}

// 只由父测试启动：发布非秘密身份后阻塞 stdin，不访问网络、注册表、配置或其他进程。
#[test]
#[ignore = "仅由宿主异常退出测试作为子进程调用"]
fn runtimeOwnerFixture() {
    println!(
        "LEASE {}",
        serde_json::to_string(&currentIdentity().unwrap()).unwrap()
    );
    std::io::stdout().flush().unwrap();
    let mut release = [0u8; 1];
    std::io::stdin().read(&mut release).unwrap();
}
