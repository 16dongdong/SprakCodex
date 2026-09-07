//! 仅保留模型、响应标识与服务端 usage；正文、工具参数、认证字段不进入持久化对象。
use codexmanager_core::storage::RequestTokenStat;
use serde_json::Value;

pub(super) const maxEventBytes: usize = 16 * 1024 * 1024;

#[derive(Default, Clone)]
pub(super) struct UsageParser {
    pub model: Option<String>,
    pub responseId: Option<String>,
    pub usage: RequestTokenStat,
    pub problem: Option<&'static str>,
    pub terminal: bool,
    line: Vec<u8>,
    event: Vec<u8>,
    skipping: bool,
}

impl UsageParser {
    // 只接受静态诊断，禁止将上游正文或含密钥 URL 带入错误记录。
    pub fn failure(problem: &'static str) -> Self {
        Self {
            problem: Some(problem),
            ..Default::default()
        }
    }

    // 接收已解压字节；SSE 逐行装配，单事件限额而非整条流限额，尾部 usage 不受预览长度影响。
    // 超限仅标记该观测事件未知并继续寻找下一事件，不修改转发的原始字节。
    pub fn feed(&mut self, bytes: &[u8], sse: bool) {
        if !sse {
            if self.event.len().saturating_add(bytes.len()) > maxEventBytes {
                self.event.clear();
                self.skipping = true;
                self.problem = Some("观测事件超过内存限额，用量可能缺失");
            }
            if !self.skipping {
                self.event.extend_from_slice(bytes);
            }
            return;
        }
        for &byte in bytes {
            if byte == b'\n' {
                self.finishLine();
            } else if self.line.len() < maxEventBytes {
                self.line.push(byte);
            } else {
                self.skipping = true;
                self.problem = Some("观测事件超过内存限额，用量可能缺失");
            }
        }
    }

    // 处理 CRLF、多行 data 和心跳；SSE 的空行是提交边界，不按网络分包边界猜测 JSON。
    fn finishLine(&mut self) {
        if self.line.last() == Some(&b'\r') {
            self.line.pop();
        }
        if self.line.is_empty() {
            if !self.skipping {
                let event = std::mem::take(&mut self.event);
                self.json(&event);
            }
            self.event.clear();
            self.skipping = false;
        } else if let Some(payload) = self.line.strip_prefix(b"data:") {
            let payload = payload.strip_prefix(b" ").unwrap_or(payload);
            if self.event.len().saturating_add(payload.len() + 1) > maxEventBytes {
                self.skipping = true;
                self.problem = Some("观测事件超过内存限额，用量可能缺失");
            } else if !self.skipping {
                if !self.event.is_empty() {
                    self.event.push(b'\n');
                }
                self.event.extend_from_slice(payload);
            }
        }
        self.line.clear();
    }

    // JSON 响应在 EOF 提交；SSE 未以空行结束的事件不算完整，避免把截断流当成功。
    pub fn finish(&mut self, sse: bool) {
        if !sse && !self.skipping {
            let event = std::mem::take(&mut self.event);
            self.json(&event);
        }
        if sse && !self.terminal && self.problem.is_none() {
            self.problem = Some("流在终结事件之前关闭，用量可能缺失");
        }
    }

    // 读取 Responses、Chat Completions 与 WebSocket 消息的同一语义；非法 JSON 不产生虚构计数。
    pub fn json(&mut self, bytes: &[u8]) {
        if bytes == b"[DONE]" {
            self.terminal = true;
            return;
        }
        let Ok(message) = serde_json::from_slice::<Value>(bytes) else {
            return;
        };
        self.message(&message);
    }

    // usage 是累计快照，不累加 delta；缓存和推理是子集，不能再次计入 total。
    pub fn message(&mut self, message: &Value) {
        let response = message.get("response").unwrap_or(message);
        if let Some(model) = response
            .get("model")
            .and_then(Value::as_str)
            .filter(|v| validLabel(v))
        {
            self.model = Some(model.to_owned());
        }
        if let Some(identity) = response
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| v.len() <= 256)
        {
            self.responseId = Some(identity.to_owned());
        }
        let eventType = message.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(
            eventType,
            "response.completed" | "response.failed" | "response.incomplete"
        ) {
            self.terminal = true;
            if eventType != "response.completed" {
                self.problem = Some("上游响应未完整完成");
            }
        }
        let Some(usage) = response.get("usage").filter(|v| v.is_object()) else {
            return;
        };
        self.usage.input_tokens =
            counter(usage, "input_tokens").or_else(|| counter(usage, "prompt_tokens"));
        self.usage.output_tokens =
            counter(usage, "output_tokens").or_else(|| counter(usage, "completion_tokens"));
        self.usage.cached_input_tokens = usage
            .get("input_tokens_details")
            .or_else(|| usage.get("prompt_tokens_details"))
            .and_then(|details| counter(details, "cached_tokens"));
        self.usage.reasoning_output_tokens = usage
            .get("output_tokens_details")
            .or_else(|| usage.get("completion_tokens_details"))
            .and_then(|details| counter(details, "reasoning_tokens"));
        self.usage.total_tokens = counter(usage, "total_tokens").or_else(|| {
            self.usage
                .input_tokens?
                .checked_add(self.usage.output_tokens?)
        });
    }
}

// 模型是短标识而非任意正文；拒绝控制字符与超长文本，避免日志被内容字段污染。
fn validLabel(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 160
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/:".contains(&b))
}

// 服务端缺失、负数或溢出保持 None，UI 可以区分未知与真实零用量。
fn counter(usage: &Value, field: &str) -> Option<i64> {
    usage.get(field)?.as_i64().filter(|value| *value >= 0)
}

#[cfg(test)]
#[path = "../../tests/observation/usageParserTests.rs"]
mod tests;
