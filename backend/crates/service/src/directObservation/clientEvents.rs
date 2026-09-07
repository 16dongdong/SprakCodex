//! 流式读取 CLI 逐响应事件；未知正文直接跳过，客户端模型上下文与网络观测分开标记。
use super::usageParser::UsageParser;
use codexmanager_core::storage::RequestTokenStat;
use serde::{
    de::{IgnoredAny, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

// 只保存模型关联与读取位置，不保存提示词、响应正文、工具参数或认证材料。
#[derive(Default)]
pub(super) struct Journal {
    offset: u64,
    official: bool,
    thread: Option<String>,
    models: HashMap<String, String>,
}

// 会话来源与归属只取必要字段，其他元数据由 serde 忽略。
#[derive(Deserialize)]
struct Session {
    id: String,
    model_provider: Option<String>,
}
// 轮次上下文提供客户端选择的模型，不将它标成上游返回模型。
#[derive(Deserialize)]
struct Turn {
    turn_id: String,
    model: String,
}
// 可缺省字段保留未知，非零或缺失的缓存写入暂不参与估价。
#[derive(Deserialize)]
struct Usage {
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    reasoning_output_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
}
// 响应身份用于跨来源去重；用量始终绑定到同一会话和轮次。
#[derive(Deserialize)]
struct Completed {
    thread_id: String,
    turn_id: String,
    response_id: String,
    usage: Usage,
}

enum Payload {
    Session(Session),
    Turn(Turn),
    Completed(Completed),
    Ignored,
}
struct Event {
    timestamp: Option<i64>,
    payload: Payload,
}

// 只输出完成事件的必要字段，写入成功确认后才推进文件位置。
pub(super) struct Completion {
    pub parsed: UsageParser,
    pub timestamp: i64,
    pub pricingAllowed: bool,
}

impl<'de> Deserialize<'de> for Event {
    // 生产格式先写 type 再写 payload；按已知类型定向反序列化，避免相邻标签枚举缓存整段未知正文。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvelopeVisitor;
        impl<'de> Visitor<'de> for EnvelopeVisitor {
            type Value = Event;
            // 错误仅说明协议结构，不回显原始 JSON。
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("CLI 事件对象")
            }
            // 只有三个已知 payload 会构建对象，其余 JSON 使用 IgnoredAny 流式消费。
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Event, A::Error> {
                let mut kind = String::new();
                let mut timestamp = None;
                let mut payload = Payload::Ignored;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "timestamp" => {
                            let value: String = map.next_value()?;
                            timestamp = chrono::DateTime::parse_from_rfc3339(&value)
                                .ok()
                                .map(|value| value.timestamp_millis());
                        }
                        "type" => kind = map.next_value()?,
                        "payload" => {
                            payload = match kind.as_str() {
                                "session_meta" => Payload::Session(map.next_value()?),
                                "turn_context" => Payload::Turn(map.next_value()?),
                                "token_usage_record" => Payload::Completed(map.next_value()?),
                                _ => {
                                    map.next_value::<IgnoredAny>()?;
                                    Payload::Ignored
                                }
                            }
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(Event { timestamp, payload })
            }
        }
        deserializer.deserialize_map(EnvelopeVisitor)
    }
}

impl Journal {
    // 每次只读当前长度内的增量；首次重建模型关联但不回填启用时刻之前的用量，半条事件等待下次写入。
    pub(super) fn poll(
        &mut self,
        path: &Path,
        since: i64,
        report: &mut impl FnMut(Completion) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut file = File::open(path).map_err(|_| "打开客户端事件文件失败")?;
        let length = file.metadata().map_err(|_| "读取客户端事件长度失败")?.len();
        if length < self.offset {
            *self = Self::default();
        }
        let start = self.offset;
        file.seek(SeekFrom::Start(start))
            .map_err(|_| "定位客户端事件失败")?;
        let reader = BufReader::new(file.take(length - start));
        let mut stream = serde_json::Deserializer::from_reader(reader).into_iter::<Event>();
        while let Some(event) = stream.next() {
            let event = match event {
                Ok(event) => event,
                Err(error) if error.is_eof() => break,
                Err(_) => return Err("客户端事件结构不完整或格式已变化".into()),
            };
            self.consume(event, since, report)?;
            self.offset = start + stream.byte_offset() as u64;
        }
        Ok(())
    }

    // 完成事件必须属于当前会话及已知轮次；只提交服务端报告的计数，不估算缺失 Token。
    fn consume(
        &mut self,
        event: Event,
        since: i64,
        report: &mut impl FnMut(Completion) -> Result<(), String>,
    ) -> Result<(), String> {
        match event.payload {
            Payload::Session(session) => {
                self.official = session.model_provider.as_deref() == Some("openai");
                self.thread = Some(session.id);
            }
            Payload::Turn(turn) => {
                self.models.insert(turn.turn_id, turn.model);
            }
            Payload::Completed(completed) if self.official => {
                let timestamp = event.timestamp.ok_or("客户端完成事件缺少有效时间")?;
                if timestamp < since {
                    return Ok(());
                }
                if self.thread.as_deref() != Some(completed.thread_id.as_str())
                    || completed.response_id.is_empty()
                {
                    return Err("客户端完成事件归属不匹配".into());
                }
                let model = self
                    .models
                    .get(&completed.turn_id)
                    .ok_or("客户端完成事件缺少模型上下文")?;
                let usage = completed.usage;
                if usage.input_tokens < 0
                    || usage.output_tokens < 0
                    || usage.cached_input_tokens < 0
                    || usage.cached_input_tokens > usage.input_tokens
                    || usage.input_tokens.checked_add(usage.output_tokens)
                        != Some(usage.total_tokens)
                    || usage
                        .reasoning_output_tokens
                        .is_some_and(|count| count < 0 || count > usage.output_tokens)
                    || usage
                        .cache_write_input_tokens
                        .is_some_and(|count| count < 0)
                {
                    return Err("客户端用量计数不满足约束".into());
                }
                let parsed = UsageParser::reported(
                    model.clone(),
                    completed.response_id,
                    RequestTokenStat {
                        input_tokens: Some(usage.input_tokens),
                        cached_input_tokens: Some(usage.cached_input_tokens),
                        output_tokens: Some(usage.output_tokens),
                        total_tokens: Some(usage.total_tokens),
                        reasoning_output_tokens: usage.reasoning_output_tokens,
                        ..Default::default()
                    },
                );
                report(Completion {
                    parsed,
                    timestamp: timestamp / 1000,
                    pricingAllowed: usage.cache_write_input_tokens == Some(0),
                })?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/observation/clientEventTests.rs"]
mod tests;
