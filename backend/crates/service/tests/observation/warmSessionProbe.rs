//! 已运行 app-server 会话验收：首次生成在观测前完成，随后保持进程和 thread 不变启用生产扫描。
use super::super::{clientEventMonitor, recordSink::RecordSink};
use super::{super::nativeInjection, injectedClientProbe, sessionRpcPeer::SessionPeer};
use codexmanager_core::storage::Storage;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenEventW, WaitForSingleObject, SYNCHRONIZATION_SYNCHRONIZE},
    },
};

// 沿用用户原 provider 和登录；仅在测试进程禁用已有 MCP 启动，避免无工具探针启动其他本地服务。
pub(super) fn command(cli: &Path) -> Command {
    let home =
        PathBuf::from(std::env::var_os("CODEX_HOME").expect("暖会话探针要求明确的 CODEX_HOME"));
    let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
    let config = config
        .parse::<toml_edit::DocumentMut>()
        .expect("读取客户端配置结构");
    assert_eq!(
        config
            .get("model_provider")
            .and_then(|value| value.as_str())
            .unwrap_or("openai"),
        "openai",
        "探针不覆盖用户 provider"
    );
    let mut command = Command::new(cli);
    command.args(["app-server", "-c", "project_doc_max_bytes=0"]);
    if let Some(servers) = config
        .get("mcp_servers")
        .and_then(|value| value.as_table_like())
    {
        for (name, _) in servers.iter() {
            assert!(
                name.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
                "MCP 名称需要单独的参数编码支持"
            );
            command.args(["-c", &format!("mcp_servers.{name}.enabled=false")]);
        }
    }
    command
}

// 同一客户端、同一 thread 先生成再接入；不关闭它的网络连接，不重新登录，不通过创建新 thread 掩盖旧连接。
pub(super) struct Options<'a> {
    pub directory: &'a Path,
    pub certificate: &'a Path,
    pub port: u16,
    pub sink: RecordSink,
}

// 同一会话的基线和后续轮次共用原连接，事件源也使用生产实现，仅限定到本测试 thread 文件。
pub(super) fn run(command: &mut Command, options: Options<'_>) -> bool {
    let Options {
        directory,
        certificate,
        port,
        sink,
    } = options;
    let mut target = injectedClientProbe::prepare(directory, certificate, port);
    target.child = Some(command.spawn().expect("启动独立 app-server"));
    let mut peer = SessionPeer::new(target.child.as_mut().unwrap());
    peer.call("initialize", json!({"clientInfo":{"name":"observationProbe","version":"1"},"capabilities":{"experimentalApi":true,"optOutNotificationMethods":["rawResponseItem/completed","item/reasoning/textDelta","item/reasoning/summaryTextDelta","item/agentMessage/delta","item/completed"]}}));
    peer.initialized();
    let model = std::env::var("OBSERVATION_TEST_MODEL").unwrap();
    let result = peer.call("thread/start", json!({"model":model,"cwd":directory,"approvalPolicy":"never","sandbox":"read-only","ephemeral":false,"experimentalRawEvents":true}));
    assert_eq!(result["modelProvider"], "openai");
    let thread = result["thread"]["id"]
        .as_str()
        .expect("缺少 thread ID")
        .to_owned();
    let before = peer.turn(&thread);
    let suffix = format!("-{thread}.jsonl");
    let eventMonitor = clientEventMonitor::EventMonitor::start(
        clientEventMonitor::Settings {
            home: clientEventMonitor::defaultHome().unwrap(),
            since: chrono::Utc::now().timestamp_millis(),
            allow: Arc::new(move |path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
            }),
        },
        sink.clone(),
        tokio_util::sync::CancellationToken::new(),
    )
    .unwrap();
    let identity = nativeInjection::candidate(target.child.as_ref().unwrap().id()).unwrap();
    target.monitor = Some(injectedClientProbe::startMonitor(
        target.moduleDirectory.join("cphook.dll"),
        Arc::new(Mutex::new(Some(identity.clone()))),
    ));
    waitReady(identity.pid, &target.moduleDirectory.join("cphook.dll"));
    let after = peer.turn(&thread);
    let persisted = Instant::now();
    while sink
        .counters
        .written
        .load(std::sync::atomic::Ordering::Relaxed)
        < after.len() as u64
    {
        assert!(
            persisted.elapsed() < Duration::from_secs(10),
            "已有会话事件未完成持久化"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let evidence = json!({"threadId":thread,"model":result["model"],"before":before,"after":after});
    std::fs::write(
        directory.join("warmEvidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    // app-server 是长期服务，测试结束明确回收独立进程，随后关闭其输出读取线程。
    peer.closeInput();
    let success = super::waitClient(target.child.as_mut().unwrap());
    peer.joinReader();
    drop(eventMonitor);
    success
}

// 只等待已加载模块的事件，不调用注入；原生加载动作必须来自正在运行的生产扫描器。
fn waitReady(pid: u32, module: &Path) {
    let name = HSTRING::from(cpcommon::hook_ready::event_name(pid, module));
    let started = Instant::now();
    loop {
        if let Ok(event) = unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
            let ready = unsafe { WaitForSingleObject(event, 0) == WAIT_OBJECT_0 };
            unsafe {
                CloseHandle(event).unwrap();
            }
            if ready {
                return;
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "暖会话模块未就绪"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// 第一轮作为启动前基线，不计入观测；第二轮每个响应必须有真实记录与准确 usage，不能只检查模块就绪。
pub(super) fn verify(directory: &Path, storage: &Storage) {
    let evidence: Value =
        serde_json::from_slice(&std::fs::read(directory.join("warmEvidence.json")).unwrap())
            .unwrap();
    let records = storage.list_request_logs(None, 100).unwrap();
    let after = evidence["after"].as_array().unwrap();
    assert_eq!(records.len(), after.len(), "启用前的基线请求不应重复回填");
    for response in after {
        let id = response["responseId"].as_str().unwrap();
        let (trace, _) = super::super::responseIdentity::traces(id);
        let record = records
            .iter()
            .find(|record| record.trace_id.as_deref() == Some(trace.as_str()))
            .expect("已有会话第二轮成功但缺少对应观测记录");
        assert_eq!(
            record.request_type.as_deref(),
            Some(codexmanager_core::storage::observationClientRequestType)
        );
        assert!(
            record.status_code.is_none()
                && record.upstream_url.is_none()
                && record.duration_ms.is_none(),
            "客户端事件不能伪造 HTTP 观测字段"
        );
        assert!(record.key_id.is_none() && record.account_id.is_none());
        assert_eq!(record.model.as_deref(), evidence["model"].as_str());
        assert_eq!(
            record.input_tokens,
            response["usage"]["inputTokens"].as_i64()
        );
        assert_eq!(
            record.output_tokens,
            response["usage"]["outputTokens"].as_i64()
        );
        assert_eq!(
            record.cached_input_tokens,
            response["usage"]["cachedInputTokens"].as_i64()
        );
        assert_eq!(
            record.total_tokens,
            response["usage"]["totalTokens"].as_i64()
        );
        assert_eq!(
            record.reasoning_output_tokens,
            response["usage"]["reasoningOutputTokens"].as_i64()
        );
    }
    assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
    assert_eq!(
        (1..=records.len() as i64)
            .filter(|id| storage.get_charge_snapshot_v2(*id).unwrap().is_some())
            .count(),
        after.len()
    );
    println!("已运行会话：第二轮逐响应核对通过，生成数={}", after.len());
}
