//! 官方真实流量探针：区分显式代理与进程内接入；独立子进程同步点不代替常驻扫描、既有连接与重启验收。
use super::*;
use codexmanager_core::storage::Storage;
#[cfg(windows)]
#[path = "injectedClientProbe.rs"]
mod injectedClientProbe;
#[path = "nativeUsageVerifier.rs"]
mod nativeUsageVerifier;
#[cfg(windows)]
#[path = "sessionRpcPeer.rs"]
mod sessionRpcPeer;
#[cfg(windows)]
#[path = "warmSessionProbe.rs"]
mod warmSessionProbe;
use std::{
    fs::File,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const requestDeadline: Duration = Duration::from_secs(120);
const pollInterval: Duration = Duration::from_millis(100);

// 使用调用方指定的独立目录和现有 CLI 登录，官方 provider/base_url 不覆盖；结果仅输出数值和静态结论。
// 忽略测试必须显式提供路径，普通 cargo test 不使用正式凭据、不访问外网。
#[test]
#[ignore = "需要 OBSERVATION_TEST_CLI、OBSERVATION_TEST_DIRECTORY 和 OBSERVATION_TEST_MODEL，产生一次真实官方请求"]
fn officialTransportRecordsUsage() {
    env_logger::Builder::new()
        .filter_module(
            "codexmanager_service::directObservation",
            log::LevelFilter::Debug,
        )
        .is_test(true)
        .try_init()
        .unwrap();
    let cli = PathBuf::from(std::env::var_os("OBSERVATION_TEST_CLI").expect("缺少测试 CLI 路径"));
    let directory =
        PathBuf::from(std::env::var_os("OBSERVATION_TEST_DIRECTORY").expect("缺少独立测试目录"));
    let model = std::env::var("OBSERVATION_TEST_MODEL").expect("缺少真实模型名");
    let protocol =
        std::env::var("OBSERVATION_TEST_PROTOCOL").unwrap_or_else(|_| "websocket".into());
    assert!(
        matches!(protocol.as_str(), "websocket" | "sse"),
        "测试协议必须为 websocket 或 sse"
    );
    let websocket = protocol == "websocket";
    let captureMode =
        std::env::var("OBSERVATION_TEST_CAPTURE_MODE").unwrap_or_else(|_| "explicit".into());
    assert!(
        matches!(
            captureMode.as_str(),
            "explicit" | "injected" | "monitored" | "warm" | "runtime"
        ),
        "抓取方式必须为 explicit、injected、monitored、warm 或 runtime"
    );
    let injected = captureMode == "injected";
    let native = captureMode != "explicit";
    let warm = matches!(captureMode.as_str(), "warm" | "runtime");
    assert!(directory.is_absolute(), "测试目录必须为绝对路径");
    std::fs::create_dir(&directory).expect("测试目录必须是本次新建目录");
    let database = directory.join("observation.db");
    let storage = Storage::open(&database).unwrap();
    storage.init().unwrap();
    let authority = certificateAuthority::Authority::create(observationHosts).unwrap();
    let certificate = directory.join("authority.pem");
    std::fs::write(&certificate, &authority.pem).unwrap();
    let (sink, databaseWorker) = recordSink::RecordSink::start(database).unwrap();
    let mut warmSink = warm.then(|| sink.clone());
    let counters = sink.counters.clone();
    let cancel = CancellationToken::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    // 不调用 start()，防止这个协议探针的注入扫描触及当前编辑器和桌面会话。
    let (listener, engine) = runtime.block_on(async {
        let mut engine = transport::Engine::new(
            authority,
            sink,
            std::env::var("OBSERVATION_TEST_UPSTREAM_PROXY").ok(),
            cancel.clone(),
        )
        .await
        .unwrap();
        if !websocket {
            // SSE 用例让观测器的 WSS 上游严格拒绝证书，触发 CLI 自身的 HTTP 回退；不覆盖官方内置 provider。
            // HTTP 的公网证书校验保持原样，生成请求实际走哪种协议仍由入库记录断言。
            let strictTls = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
            engine.websocketTls = Arc::new(strictTls);
        }
        let listener = loopbackListeners::LoopbackListeners::bind().await.unwrap();
        (listener, Arc::new(engine))
    });
    let proxy = format!("http://127.0.0.1:{}", listener.port());
    let serving = runtime.spawn(transport::serve(listener, engine));
    let events = directory.join("clientEvents.jsonl");
    let mut command = Command::new(&cli);
    if warm {
        #[cfg(windows)]
        {
            command = warmSessionProbe::command(&cli);
        }
        #[cfg(not(windows))]
        {
            panic!("暖会话探针仅支持 Windows");
        }
    } else {
        command
            .args([
                "exec",
                "--ignore-user-config",
                "--json",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "-c",
                "project_doc_max_bytes=0",
                "-m",
                &model,
                "-C",
            ])
            .arg(&directory)
            // 省略位置参数会输出 stdin 等待标记；显式 "-" 在官方 CLI 中静默等待，不适合作为同步证据。
            .args((!injected).then_some("只回复 OBSERVATION_OK，不要调用工具或读取文件。"));
    }
    command
        .stdin(if injected || warm {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if warm {
            Stdio::piped()
        } else {
            Stdio::from(File::create(&events).unwrap())
        })
        .stderr(File::create(directory.join("clientDiagnostics.log")).unwrap());
    // 显式代理仅属于旧协议探针；原生注入模式原样继承代理与 CA 环境，禁止把改环境当自动捕获。
    if !native {
        for variable in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env(variable, &proxy);
        }
        command
            .env("NO_PROXY", "localhost,127.0.0.1,::1")
            .env("no_proxy", "localhost,127.0.0.1,::1")
            .env("CODEX_CA_CERTIFICATE", &certificate);
    }
    command.env("RUST_LOG", "error");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const noWindow: u32 = 0x08000000;
        command.creation_flags(noWindow);
    }
    let result = if native {
        #[cfg(windows)]
        {
            if warm {
                warmSessionProbe::run(
                    &mut command,
                    warmSessionProbe::Options {
                        directory: &directory,
                        certificate: &certificate,
                        port: listenerPort(&proxy),
                        sink: warmSink.take().unwrap(),
                    },
                )
            } else {
                injectedClientProbe::run(
                    &mut command,
                    &directory,
                    &certificate,
                    listenerPort(&proxy),
                )
            }
        }
        #[cfg(not(windows))]
        {
            panic!("原生注入探针仅支持 Windows");
        }
    } else {
        runClient(&mut command)
    };
    cancel.cancel();
    runtime.block_on(serving).unwrap();
    runtime.shutdown_timeout(Duration::from_secs(5));
    databaseWorker.join().unwrap();
    std::fs::remove_file(certificate).unwrap();
    assert!(result, "官方 CLI 请求失败，检查隔离目录中的诊断文件");
    assert_eq!(counters.errors.load(Ordering::Relaxed), 0);
    if warm {
        #[cfg(windows)]
        warmSessionProbe::verify(&directory, &storage);
    } else {
        let stdout = std::fs::read_to_string(events).unwrap();
        verifyCapturedRecords(&stdout, &storage, websocket);
    }
}

// 用独立客户端事件核对每个网络响应及生成快照，失败握手与预热保留记录但不充当生成用量。
fn verifyCapturedRecords(stdout: &str, storage: &Storage, websocket: bool) {
    let protocol = if websocket { "websocket" } else { "sse" };
    let completions: Vec<_> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["type"] == "turn.completed")
        .collect();
    assert!(!completions.is_empty(), "缺少 CLI 完整终态");
    let threadIds: std::collections::HashSet<_> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["type"] == "thread.started")
        .map(|event| {
            event["thread_id"]
                .as_str()
                .expect("缺少 CLI 线程标识")
                .to_owned()
        })
        .collect();
    assert_eq!(
        threadIds.len(),
        completions.len(),
        "每个测试 CLI 应有独立完整终态"
    );
    let native: Vec<_> = threadIds
        .iter()
        .flat_map(|threadId| {
            nativeUsageVerifier::readProbeUsage(threadId).expect("独立逐请求核对失败")
        })
        .collect();
    let records = storage.list_request_logs(None, 100).unwrap();
    assert!(!records.is_empty(), "官方请求成功但观测未入库");
    let mut input = 0;
    let mut output = 0;
    let mut cached = 0;
    let mut prewarmRequests = 0;
    let mut generationRequests = 0;
    let mut failedHandshakes = 0;
    for record in &records {
        assert_eq!(record.gateway_mode.as_deref(), Some("directObservation"));
        assert!(
            record.key_id.is_none() && record.account_id.is_none(),
            "观测不应绑定账号池或平台密钥"
        );
        if record.request_type.as_deref() == Some(websocketObservation::handshakeProtocol) {
            assert!(!websocket, "正常 WebSocket 验收中出现了握手失败");
            assert_eq!(record.status_code, Some(502));
            assert!(
                record.input_tokens.is_none() && record.output_tokens.is_none(),
                "握手失败不应生成用量"
            );
            failedHandshakes += 1;
            continue;
        }
        assert_eq!(record.status_code, Some(200));
        if record.request_type.as_deref()
            == Some(codexmanager_core::storage::observationPrewarmRequestType)
        {
            prewarmRequests += 1;
            continue;
        }
        generationRequests += 1;
        assert_eq!(
            record.request_type.as_deref(),
            Some(if websocket { "websocket" } else { "http" }),
            "实际传输与所选验收协议不一致"
        );
        input += record.input_tokens.expect("输入用量缺失");
        output += record.output_tokens.expect("输出用量缺失");
        cached += record.cached_input_tokens.expect("缓存用量缺失");
    }
    // 常驻验收包含前后两个独立 CLI，累计值与每个 response_id 同时核对，不能只验证其中一次成功。
    let reportedSum = |field: &str| {
        completions
            .iter()
            .map(|event| event["usage"][field].as_i64().expect("终态用量缺失"))
            .sum::<i64>()
    };
    assert_eq!(reportedSum("input_tokens"), input);
    assert_eq!(reportedSum("output_tokens"), output);
    assert_eq!(reportedSum("cached_input_tokens"), cached);
    assert!(generationRequests > 0, "只有预热没有真实生成请求");
    assert_eq!(
        native.len(),
        generationRequests,
        "网络捕获与客户端逐请求数量不一致"
    );
    for reported in native {
        let (trace, aliases) = responseIdentity::traces(&reported.responseId);
        let matched = records
            .iter()
            .find(|record| {
                record
                    .trace_id
                    .as_ref()
                    .is_some_and(|id| id == &trace || aliases.contains(id))
            })
            .expect("网络记录没有对应的真实响应 ID");
        assert_eq!(matched.input_tokens, Some(reported.usage.input_tokens));
        assert_eq!(
            matched.cached_input_tokens,
            Some(reported.usage.cached_input_tokens)
        );
        assert_eq!(matched.output_tokens, Some(reported.usage.output_tokens));
        assert_eq!(matched.total_tokens, Some(reported.usage.total_tokens));
        assert_eq!(
            matched.reasoning_output_tokens,
            Some(reported.usage.reasoning_output_tokens)
        );
        assert_eq!(
            reported.usage.cache_write_input_tokens, 0,
            "当前观测存储尚未表达非零缓存写入，需补齐后再通过验收"
        );
        assert_eq!(matched.model.as_deref(), Some(reported.model.as_str()));
    }
    // 独立数据库只包含本次观测记录；每个生成响应都应有快照，预热不按生成价格虚构费用。
    let snapshots = (1..=records.len() as i64)
        .filter(|id| storage.get_charge_snapshot_v2(*id).unwrap().is_some())
        .count();
    assert_eq!(snapshots, generationRequests);
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
    println!("官方直连协议探针：协议={protocol}，请求={}，生成={generationRequests}，预热={prewarmRequests}，握手失败={failedHandshakes}，输入={input}，缓存={cached}，输出={output}，费用快照={snapshots}，钱包扣费=0；常驻扫描、既有连接和完整重启另行验收", records.len());
}

// 回放本次探针已经保存的证据，验证新增断言而不重复消耗真实请求；不把回放计作一次新的网络测试。
#[test]
#[ignore = "需要 OBSERVATION_TEST_DIRECTORY 指向已完成的探针目录"]
fn verifyPreviouslyCapturedRecords() {
    let directory =
        PathBuf::from(std::env::var_os("OBSERVATION_TEST_DIRECTORY").expect("缺少探针目录"));
    let stdout = std::fs::read_to_string(directory.join("clientEvents.jsonl")).unwrap();
    let database = directory.join("observation.db");
    assert!(database.is_file(), "探针数据库缺失");
    let storage = Storage::open(database).unwrap();
    let websocket = std::env::var("OBSERVATION_TEST_PROTOCOL")
        .unwrap_or_else(|_| "websocket".into())
        == "websocket";
    verifyCapturedRecords(&stdout, &storage, websocket);
}

// 只等待本测试创建的 CLI；超时终止该子进程并回收，避免测试永久挂起或遗留真实请求。
fn runClient(command: &mut Command) -> bool {
    let mut client = command.spawn().expect("启动测试 CLI 失败");
    waitClient(&mut client)
}

// 原生与显式代理探针共用完整请求截止时间；只终止本次创建的进程，不重启已有用户会话。
fn waitClient(client: &mut std::process::Child) -> bool {
    let started = Instant::now();
    loop {
        if let Some(status) = client.try_wait().expect("读取 CLI 状态失败") {
            return status.success();
        }
        if started.elapsed() >= requestDeadline {
            client.kill().expect("终止超时 CLI 失败");
            client.wait().expect("回收超时 CLI 失败");
            return false;
        }
        std::thread::sleep(pollInterval);
    }
}

// Relay 地址由已绑定 listener 生成，解析失败属于探针内部错误，不猜默认端口。
#[cfg(windows)]
fn listenerPort(address: &str) -> u16 {
    url::Url::parse(address).unwrap().port().unwrap()
}
