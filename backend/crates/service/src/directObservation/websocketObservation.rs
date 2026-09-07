//! WebSocket 逐请求关联：预热与生成分开，原始帧不改写，仅保存非内容的请求类型和响应 usage。
use super::{
    recordSink::Exchange,
    usageParser::{maxEventBytes, UsageParser},
};
use codexmanager_core::storage::observationPrewarmRequestType;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use tokio_tungstenite::tungstenite::Message;

const maxPendingResponses: usize = 64;
const generationProtocol: &str = "websocket";
pub(super) const handshakeProtocol: &str = "websocketHandshake";

// 只反序列化控制标志，正文、工具定义与认证字段由 serde 丢弃，避免创建完整提示词 JSON 树。
#[derive(Deserialize)]
struct RequestShape {
    #[serde(rename = "type")]
    kind: String,
    generate: Option<bool>,
}

// 每个连接独占状态，按创建响应顺序绑定请求；最近终态 ID 防止重复响应消耗下一条请求的元数据。
#[derive(Default)]
pub(super) struct Observation {
    queued: VecDeque<Exchange>,
    pending: HashMap<String, (Exchange, UsageParser)>,
    completed: VecDeque<String>,
}

impl Observation {
    // 转发前记录 response.create 的开始时间；非创建消息无副作用，容量超限返回显式观测错误。
    pub fn request(&mut self, message: &Message, target: (&str, &str)) -> Result<(), ()> {
        let Some(bytes) = messageBytes(message) else {
            return Ok(());
        };
        let Ok(shape) = serde_json::from_slice::<RequestShape>(bytes) else {
            return Ok(());
        };
        if shape.kind != "response.create" {
            return Ok(());
        }
        if self.queued.len() + self.pending.len() >= maxPendingResponses {
            return Err(());
        }
        let protocol = if shape.generate == Some(false) {
            observationPrewarmRequestType
        } else {
            generationProtocol
        };
        self.queued
            .push_back(Exchange::new(target.0, target.1, protocol));
        Ok(())
    }

    // 响应标识绑定首个尚未获得 response.created 的请求；只在真实终态返回可写库记录。
    pub fn response(
        &mut self,
        message: &Message,
    ) -> Result<Option<(Exchange, UsageParser, u16)>, ()> {
        let Some(bytes) = messageBytes(message) else {
            return Ok(None);
        };
        if bytes.len() > maxEventBytes {
            return Err(());
        }
        let Ok(event) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return Ok(None);
        };
        if event.get("type").and_then(|value| value.as_str()) == Some("error") {
            let status = event
                .get("status")
                .and_then(|value| value.as_u64())
                .filter(|status| (400..=599).contains(status))
                .map(|status| status as u16)
                .unwrap_or(502);
            return Ok(self.queued.pop_front().map(|exchange| {
                (
                    exchange,
                    UsageParser::failure("WebSocket 上游拒绝请求"),
                    status,
                )
            }));
        }
        let Some(identity) = event
            .get("response")
            .and_then(|response| response.get("id"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty() && value.len() <= 256)
        else {
            return Ok(None);
        };
        if self.completed.iter().any(|completed| completed == identity) {
            return Ok(None);
        }
        if !self.pending.contains_key(identity) {
            let exchange = self.queued.pop_front().ok_or(())?;
            self.pending
                .insert(identity.into(), (exchange, UsageParser::default()));
        }
        let (_, parsed) = self.pending.get_mut(identity).ok_or(())?;
        parsed.message(&event);
        if !parsed.terminal {
            return Ok(None);
        }
        let (exchange, parsed) = self.pending.remove(identity).ok_or(())?;
        if self.completed.len() == maxPendingResponses {
            self.completed.pop_front();
        }
        self.completed.push_back(identity.into());
        let status = if parsed.problem.is_some() { 502 } else { 200 };
        Ok(Some((exchange, parsed, status)))
    }

    // 连接结束后提交所有未完成请求，包括只发出 create 尚未收到 response.created 的情况。
    pub fn unfinished(self) -> impl Iterator<Item = (Exchange, UsageParser)> {
        self.pending.into_values().chain(
            self.queued
                .into_iter()
                .map(|exchange| (exchange, UsageParser::default())),
        )
    }
}

// WebSocket 控制帧不参与用量 JSON 解析，文本与二进制数据帧共享相同的消息边界。
fn messageBytes(message: &Message) -> Option<&[u8]> {
    match message {
        Message::Text(text) => Some(text.as_bytes()),
        Message::Binary(bytes) => Some(bytes),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/observation/websocketObservationTests.rs"]
mod tests;
