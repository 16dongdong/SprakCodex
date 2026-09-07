//! 完整宿主进程重启验收：同一个官方 app-server 跨两个观测宿主继续生成，数据和签名身份使用独立目录。
use super::super::*;
use super::{sessionRpcPeer::SessionPeer, waitClient, warmSessionProbe};
use codexmanager_core::storage::Storage;
use serde_json::{json, Value};
use std::{
    io::{BufRead, Write},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

// 所有子进程均由本探针创建；异常路径也回收，避免错误断言留下后台服务。
struct OwnedChild(Child);
impl Drop for OwnedChild {
    // 已退出进程只回收状态，仍运行的测试子进程才终止；不按进程名操作用户程序。
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
        }
        self.0.wait().unwrap();
    }
}

// 只对指定测试身份恢复生产运行期，过滤发生在进程打开及事件内容读取之前，不接触其他会话。
#[test]
#[ignore = "由宿主重启父测试提供隔离目录与真实进程身份"]
fn hostWorker() {
    let configPath =
        PathBuf::from(std::env::var_os("OBSERVATION_HOST_FIXTURE").expect("缺少宿主夹具"));
    let config: Value = serde_json::from_slice(&std::fs::read(&configPath).unwrap()).unwrap();
    let directory = configPath.parent().unwrap();
    crate::storage_helpers::initialize_storage().unwrap();
    let expected = processInjector::ProcessCandidate {
        pid: config["pid"].as_u64().unwrap() as u32,
        createdAt: config["createdAt"].as_u64().unwrap(),
        executable: PathBuf::from(config["executable"].as_str().unwrap()),
    };
    let suffix = format!("-{}.jsonl", config["threadId"].as_str().unwrap());
    let scope = RuntimeScope {
        select: Arc::new(move || {
            Ok(processInjector::findCandidates()?
                .into_iter()
                .filter(|candidate| candidate == &expected)
                .collect())
        }),
        allowEvents: Arc::new(move |path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
        }),
    };
    if config["restore"] == true {
        restoreUsing(|| startScoped(scope)).unwrap();
    } else {
        startScoped(scope).unwrap();
    }
    let state = status().unwrap();
    runtimePaths::writeAtomically(
        &directory.join(config["ready"].as_str().unwrap()),
        &serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    match line.trim() {
        "shutdown" => {
            shutdownRuntime().unwrap();
        }
        "disable" => {
            stop().unwrap();
        }
        _ => panic!("宿主夹具收到无效关闭命令"),
    }
}

// 每轮是新的操作系统进程；公开就绪文件原子发布，父测试不把存在但未写完的文件当成成功。
fn startHost(directory: &Path, config: &Value) -> (OwnedChild, Value) {
    let phase = config["phase"].as_u64().unwrap();
    let configPath = directory.join(format!("host{phase}.json"));
    std::fs::write(&configPath, serde_json::to_vec(config).unwrap()).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "directObservation::liveDirectTests::hostRestartProbe::hostWorker",
            "--ignored",
            "--nocapture",
        ])
        .env("OBSERVATION_HOST_FIXTURE", &configPath)
        .env("CODEXMANAGER_DB_PATH", directory.join("observation.db"))
        .env(
            "CODEXMANAGER_OBSERVATION_DLL",
            directory.join("module/cphook.dll"),
        )
        .env("CODEX_HOME", directory.join("observerHome"))
        .stdin(Stdio::piped())
        .stdout(std::fs::File::create(directory.join(format!("host{phase}.log"))).unwrap())
        .stderr(Stdio::null());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let mut child = OwnedChild(command.spawn().expect("启动独立观测宿主"));
    let ready = directory.join(config["ready"].as_str().unwrap());
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "观测宿主在就绪前退出"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "观测宿主启动超时"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
    (child, state)
}

// 正常退出必须由宿主关闭入口完成；父进程等待退出后才启动下一宿主，强制终止不作为通过结果。
fn stopHost(host: &mut OwnedChild, disable: bool) {
    host.0
        .stdin
        .take()
        .unwrap()
        .write_all(if disable { b"disable\n" } else { b"shutdown\n" })
        .unwrap();
    assert!(waitClient(&mut host.0), "观测宿主未正常退出");
}

// 比较公开 SPKI 指纹而不是随机签发字段；跨宿主进程必须复用同一签名公钥，不输出私钥或密文。
fn publicIdentity(directory: &Path) -> String {
    use rustls::pki_types::{pem::PemObject, CertificateDer};
    use sha2::{Digest, Sha256};
    let pem = std::fs::read(directory.join("observationAuthority.pem")).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(&pem) {
        roots.add(certificate.unwrap()).unwrap();
    }
    assert_eq!(roots.roots.len(), 1);
    format!(
        "{:x}",
        Sha256::digest(roots.roots[0].subject_public_key_info.as_ref())
    )
}

// 在下一宿主启动前检查旧端口的两个地址族均释放，不把一个地址族关闭当作完整清理。
fn verifyListenerClosed(state: &Value) {
    let Some(address) = state["proxyUrl"].as_str() else {
        return;
    };
    let port = url::Url::parse(address).unwrap().port().unwrap();
    for ip in [
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    ] {
        assert!(
            std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::new(ip, port),
                Duration::from_millis(200)
            )
            .is_err(),
            "宿主退出后旧端口仍在监听"
        );
    }
}

// 逐响应等待持久化，再比对模型、用量及费用；仅同一 thread 的完成事件进入本测试数据库。
fn verifyRows(directory: &Path, expected: &[Value], model: &str) {
    let storage = Storage::open(directory.join("observation.db")).unwrap();
    let started = Instant::now();
    loop {
        let records = storage.list_request_logs(None, 100).unwrap();
        if records.len() == expected.len() {
            for response in expected {
                let (trace, _) = responseIdentity::traces(response["responseId"].as_str().unwrap());
                let record = records
                    .iter()
                    .find(|row| row.trace_id.as_deref() == Some(&trace))
                    .expect("重启后响应标识不匹配");
                assert_eq!(record.model.as_deref(), Some(model));
                assert_eq!(
                    record.input_tokens,
                    response["usage"]["inputTokens"].as_i64()
                );
                assert_eq!(
                    record.cached_input_tokens,
                    response["usage"]["cachedInputTokens"].as_i64()
                );
                assert_eq!(
                    record.output_tokens,
                    response["usage"]["outputTokens"].as_i64()
                );
                assert_eq!(
                    record.total_tokens,
                    response["usage"]["totalTokens"].as_i64()
                );
                assert_eq!(
                    record.reasoning_output_tokens,
                    response["usage"]["reasoningOutputTokens"].as_i64()
                );
                assert!(record.key_id.is_none() && record.account_id.is_none());
                assert_eq!(
                    record.request_type.as_deref(),
                    Some(codexmanager_core::storage::observationClientRequestType)
                );
                assert!(
                    record.status_code.is_none()
                        && record.upstream_url.is_none()
                        && record.duration_ms.is_none()
                );
            }
            assert_eq!(
                (1..=records.len() as i64)
                    .filter(|id| storage.get_charge_snapshot_v2(*id).unwrap().is_some())
                    .count(),
                expected.len()
            );
            assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "宿主重启记录未完成持久化"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// 基线、启用、完整进程重启、主动停用共用原 app-server 和 thread；不覆盖官方 provider 或原登录目录。
#[test]
#[ignore = "需要官方 CLI、生产 DLL、模型与全新目录，产生真实官方请求"]
fn existingSessionSurvivesHostRestart() {
    let directory = PathBuf::from(std::env::var_os("OBSERVATION_TEST_DIRECTORY").unwrap());
    assert!(directory.is_absolute());
    std::fs::create_dir(&directory).unwrap();
    std::fs::create_dir(directory.join("module")).unwrap();
    std::fs::copy(
        std::env::var_os("OBSERVATION_TEST_NETWORK_DLL").unwrap(),
        directory.join("module/cphook.dll"),
    )
    .unwrap();
    let cli = PathBuf::from(std::env::var_os("OBSERVATION_TEST_CLI").unwrap());
    let model = std::env::var("OBSERVATION_TEST_MODEL").unwrap();
    let mut command = warmSessionProbe::command(&cli);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("RUST_LOG", "error");
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let mut client = OwnedChild(command.spawn().unwrap());
    let mut peer = SessionPeer::new(&mut client.0);
    peer.call("initialize", json!({"clientInfo":{"name":"hostRestartProbe","version":"1"},"capabilities":{"experimentalApi":true,"optOutNotificationMethods":["rawResponseItem/completed","item/reasoning/textDelta","item/reasoning/summaryTextDelta","item/agentMessage/delta","item/completed"]}}));
    peer.initialized();
    let result = peer.call("thread/start", json!({"model":model,"cwd":directory,"approvalPolicy":"never","sandbox":"read-only","ephemeral":false,"experimentalRawEvents":true}));
    assert_eq!(result["modelProvider"], "openai");
    let thread = result["thread"]["id"].as_str().unwrap();
    let baseline = peer.turn(thread);
    let identity = nativeInjection::candidate(client.0.id()).unwrap();
    let mut expected = Vec::new();
    let mut hosts = Vec::new();
    let mut signingIdentity = None;
    for phase in 0..3 {
        let config = json!({"phase":phase,"ready":format!("ready{phase}.json"),"restore":phase!=0,"pid":identity.pid,"createdAt":identity.createdAt,"executable":identity.executable,"threadId":thread});
        let (mut host, state) = startHost(&directory, &config);
        assert_eq!(state["running"], phase != 2);
        hosts.push(json!({"pid":host.0.id(),"createdAt":nativeInjection::candidate(host.0.id()).unwrap().createdAt,"state":state}));
        if phase != 2 {
            let fingerprint = publicIdentity(&directory);
            if let Some(previous) = &signingIdentity {
                assert_eq!(previous, &fingerprint, "宿主重启改变了观测签名身份");
            }
            signingIdentity = Some(fingerprint);
            warmSessionProbe::waitReady(identity.pid, &directory.join("module/cphook.dll"));
            expected.extend(peer.turn(thread));
            verifyRows(&directory, &expected, &model);
        }
        stopHost(&mut host, phase == 1);
        verifyListenerClosed(&state);
        assert!(
            client.0.try_wait().unwrap().is_none(),
            "宿主退出不应结束原 CLI"
        );
    }
    let afterDisable = peer.turn(thread);
    verifyRows(&directory, &expected, &model);
    peer.closeInput();
    assert!(waitClient(&mut client.0));
    peer.joinReader();
    let evidence = json!({"threadId":thread,"signingFingerprint":signingIdentity,"hosts":hosts,"baseline":baseline,"observed":expected,"afterDisable":afterDisable});
    std::fs::write(
        directory.join("restartEvidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    // 客户端退出后才删除其加载模块；只清理本测试的普通文件，不操作安装目录或用户会话文件。
    for entry in std::fs::read_dir(directory.join("module")).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::remove_file(entry.path()).unwrap();
    }
    std::fs::remove_dir(directory.join("module")).unwrap();
    std::fs::remove_file(directory.join("observationAuthority.dpapi")).unwrap();
    std::fs::remove_file(directory.join("observationAuthority.lock")).unwrap();
    std::fs::remove_dir(directory.join("observerHome/sessions")).unwrap();
    std::fs::remove_dir(directory.join("observerHome")).unwrap();
    println!("完整宿主重启通过：原 CLI/thread 保持、两轮用量和费用对应、主动停用后不恢复");
}
