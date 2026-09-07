//! 显式运行的官方流量探针：只验 TLS/协议/入库边界，不把按进程配置代理当成自动进程接管验收。
use super::*;
use codexmanager_core::storage::Storage;
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
    assert!(directory.is_absolute(), "测试目录必须为绝对路径");
    std::fs::create_dir(&directory).expect("测试目录必须是本次新建目录");
    let database = directory.join("observation.db");
    let storage = Storage::open(&database).unwrap();
    storage.init().unwrap();
    let authority = certificateAuthority::Authority::create(observationHosts).unwrap();
    let certificate = directory.join("authority.pem");
    std::fs::write(&certificate, &authority.pem).unwrap();
    let (sink, databaseWorker) = recordSink::RecordSink::start(database).unwrap();
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
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        (listener, Arc::new(engine))
    });
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let serving = runtime.spawn(transport::serve(listener, engine));
    let events = directory.join("clientEvents.jsonl");
    let mut command = Command::new(cli);
    command
        .args([
            "exec",
            "--ignore-user-config",
            "--ephemeral",
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
        .arg("只回复 OBSERVATION_OK，不要调用工具或读取文件。")
        .stdin(Stdio::null())
        .stdout(File::create(&events).unwrap())
        .stderr(File::create(directory.join("clientDiagnostics.log")).unwrap());
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
        .env("CODEX_CA_CERTIFICATE", &certificate)
        .env("RUST_LOG", "error");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const noWindow: u32 = 0x08000000;
        command.creation_flags(noWindow);
    }
    let result = runClient(&mut command);
    cancel.cancel();
    runtime.block_on(serving).unwrap();
    runtime.shutdown_timeout(Duration::from_secs(5));
    databaseWorker.join().unwrap();
    std::fs::remove_file(certificate).unwrap();
    assert!(result, "官方 CLI 请求失败，检查隔离目录中的诊断文件");
    let stdout = std::fs::read_to_string(events).unwrap();
    let completion = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["type"] == "turn.completed")
        .expect("缺少 CLI 完整终态");
    let records = storage.list_request_logs(None, 100).unwrap();
    assert!(!records.is_empty(), "官方请求成功但观测未入库");
    assert_eq!(counters.errors.load(Ordering::Relaxed), 0);
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
    assert_eq!(completion["usage"]["input_tokens"], input);
    assert_eq!(completion["usage"]["output_tokens"], output);
    assert_eq!(completion["usage"]["cached_input_tokens"], cached);
    assert!(generationRequests > 0, "只有预热没有真实生成请求");
    // 独立数据库只包含本次观测记录；每个生成响应都应有快照，预热不按生成价格虚构费用。
    let snapshots = (1..=records.len() as i64)
        .filter(|id| storage.get_charge_snapshot_v2(*id).unwrap().is_some())
        .count();
    assert_eq!(snapshots, generationRequests);
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
    println!("官方直连协议探针：协议={protocol}，请求={}，生成={generationRequests}，预热={prewarmRequests}，握手失败={failedHandshakes}，输入={input}，缓存={cached}，输出={output}，费用快照={snapshots}，钱包扣费=0；自动接管未由此探针验证", records.len());
}

// 只等待本测试创建的 CLI；超时终止该子进程并回收，避免测试永久挂起或遗留真实请求。
fn runClient(command: &mut Command) -> bool {
    let mut client = command.spawn().expect("启动测试 CLI 失败");
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
