//! 仅用于独立 app-server 验收，保留完成用量与 RPC 结果，不把消息正文、工具参数或推理写入证据。
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin},
    sync::mpsc,
    time::{Duration, Instant},
};

const deadline: Duration = Duration::from_secs(120);

pub(super) struct SessionPeer {
    input: Option<ChildStdin>,
    messages: mpsc::Receiver<Value>,
    queued: VecDeque<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    nextId: u64,
}

impl SessionPeer {
    // 接管本测试子进程的管道；读线程只转交有限种消息，其他原始内容读取后立即释放。
    pub(super) fn new(child: &mut Child) -> Self {
        let input = child.stdin.take().expect("app-server 输入管道");
        let output = child.stdout.take().expect("app-server 输出管道");
        let (sender, messages) = mpsc::sync_channel(64);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let line = line.expect("读取 app-server 管道");
                let event: Value =
                    serde_json::from_str(&line).expect("app-server 返回非 JSON 消息");
                let relevant = event.get("id").is_some()
                    || matches!(
                        event["method"].as_str(),
                        Some("rawResponse/completed" | "turn/completed")
                    );
                if relevant && sender.send(event).is_err() {
                    break;
                }
            }
        });
        Self {
            input: Some(input),
            messages,
            queued: VecDeque::new(),
            reader: Some(reader),
            nextId: 0,
        }
    }

    // RPC 请求只由测试构造；失败仅输出方法及错误码，不输出可能包含服务数据的错误正文。
    pub(super) fn call(&mut self, method: &str, params: Value) -> Value {
        self.nextId += 1;
        let id = self.nextId;
        self.write(&json!({"id":id,"method":method,"params":params}));
        let started = Instant::now();
        loop {
            let event = self
                .messages
                .recv_timeout(deadline.saturating_sub(started.elapsed()))
                .expect("app-server RPC 等待超时");
            if event["id"] == id && event.get("method").is_none() {
                assert!(
                    event.get("error").is_none(),
                    "app-server RPC {method} 失败，错误码={}",
                    event["error"]["code"]
                );
                return event["result"].clone();
            }
            assert!(event.get("id").is_none(), "测试期间出现非预期的客户端请求");
            self.queued.push_back(event);
        }
    }

    // 一轮仅发送固定文字、不请求工具；收集该 turn 的逐响应 usage，而不是整会话累计值。
    pub(super) fn turn(&mut self, thread: &str) -> Vec<Value> {
        let result = self.call("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"只回复 OBSERVATION_OK，不要调用工具或读取文件。"}]}));
        let turn = result["turn"]["id"]
            .as_str()
            .expect("缺少 turn ID")
            .to_owned();
        let started = Instant::now();
        let mut responses = Vec::new();
        loop {
            let event = match self.queued.pop_front() {
                Some(event) => event,
                None => self
                    .messages
                    .recv_timeout(deadline.saturating_sub(started.elapsed()))
                    .expect("app-server 生成等待超时"),
            };
            assert!(event.get("id").is_none(), "测试期间出现非预期的客户端请求");
            let payload = &event["params"];
            if payload["threadId"] != thread {
                continue;
            }
            match event["method"].as_str() {
                Some("rawResponse/completed") if payload["turnId"] == turn => {
                    responses
                        .push(json!({"responseId":payload["responseId"],"usage":payload["usage"]}));
                }
                Some("turn/completed") if payload["turn"]["id"] == turn => {
                    assert_eq!(
                        payload["turn"]["status"], "completed",
                        "app-server 轮次未成功"
                    );
                    assert!(!responses.is_empty(), "成功轮次缺少逐响应完成用量");
                    return responses;
                }
                _ => {}
            }
        }
    }

    // 初始化完成通知与正常请求共用序列化器，不插入额外协议内容。
    pub(super) fn initialized(&mut self) {
        self.write(&json!({"method":"initialized"}));
    }

    // 只写 JSON 行，不记录请求副本；所有调用方均使用当前测试构造的无秘密参数。
    fn write(&mut self, event: &Value) {
        let input = self.input.as_mut().expect("app-server 输入已关闭");
        serde_json::to_writer(&mut *input, event).expect("写入 app-server 请求");
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }

    // stdin EOF 触发 app-server 自身的关闭流程，让其先回收会话和子进程。
    pub(super) fn closeInput(&mut self) {
        self.input.take();
    }

    // 调用方先结束子进程，再回收读取线程，避免把仍在运行的会话误当 EOF。
    pub(super) fn joinReader(&mut self) {
        self.reader.take().unwrap().join().unwrap();
    }
}
