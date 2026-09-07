//! 独立验收源：只读本次 CLI 创建的会话文件，用客户端逐请求 response_id/usage 核对网络观测，不启用生产日志采集。
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

// 仅反序列化核对字段，输入、模型正文、工具参数及隐藏推理不进入验证结果。
#[derive(Deserialize)]
#[serde(tag = "type", content = "payload")]
enum Event {
    #[serde(rename = "session_meta")]
    Session { model_provider: String },
    #[serde(rename = "turn_context")]
    Turn { turn_id: String, model: String },
    #[serde(rename = "token_usage_record")]
    Usage {
        thread_id: String,
        turn_id: String,
        response_id: String,
        usage: Usage,
    },
    #[serde(other)]
    Ignored,
}

// 第一遍只读取事件类型；未知 payload 由 serde 忽略，避免带内容的事件被当作 unit 变体而解析失败。
#[derive(Deserialize)]
struct EventKind {
    #[serde(rename = "type")]
    kind: String,
}

// 原生用量记录是单次请求的快照，而非整轮累计值；缺失必要字段让验收失败，不补零。
#[derive(Deserialize, Debug)]
pub(super) struct Usage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub cache_write_input_tokens: i64,
}

// 结果仅含请求标识、选用模型及计数，不保留会话中的消息内容。
pub(super) struct RequestUsage {
    pub responseId: String,
    pub model: String,
    pub usage: Usage,
}

// 仅定位本次返回的线程 ID，最多枚举相邻三日目录；不遍历其他会话内容，也不读取 auth.json。
pub(super) fn readProbeUsage(threadId: &str) -> Result<Vec<RequestUsage>, String> {
    if threadId.len() != 36
        || !threadId
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("CLI 返回的线程标识格式无效".into());
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })
        .ok_or("没有本机 CLI 会话目录")?;
    let session = findSession(&home, threadId)?;
    let file = std::fs::File::open(session).map_err(|_| "读取本次 CLI 会话失败")?;
    parse(BufReader::new(file), threadId)
}

// 会话目录按本地日期组织；只接受名字以完整线程 ID 结束的 JSONL，跨午夜也不猜测其他文件。
fn findSession(home: &Path, threadId: &str) -> Result<PathBuf, String> {
    let suffix = format!("-{threadId}.jsonl");
    let today = chrono::Local::now().date_naive();
    for dayOffset in [-1, 0, 1] {
        let day = today + chrono::Duration::days(dayOffset);
        let directory = home
            .join("sessions")
            .join(day.format("%Y/%m/%d").to_string());
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("读取会话索引目录失败".into()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| "枚举会话文件失败")?;
            if entry.file_name().to_string_lossy().ends_with(&suffix)
                && entry
                    .file_type()
                    .map_err(|_| "读取会话文件类型失败")?
                    .is_file()
            {
                return Ok(entry.path());
            }
        }
    }
    Err("本次 CLI 没有持久化逐请求会话事件".into())
}

// 维护每轮模型上下文，核对事件归属并拒绝重复响应；未知事件整体丢弃，不导出其正文。
fn parse(reader: impl BufRead, threadId: &str) -> Result<Vec<RequestUsage>, String> {
    let mut models = HashMap::new();
    let mut requests = HashMap::new();
    let mut official = false;
    for line in reader.lines() {
        let line = line.map_err(|_| "读取会话事件行失败")?;
        let kind = serde_json::from_str::<EventKind>(&line).map_err(|_| "会话事件类型不完整")?;
        let event = match kind.kind.as_str() {
            "session_meta" | "turn_context" | "token_usage_record" => {
                serde_json::from_str::<Event>(&line).map_err(|_| "会话事件格式不完整")?
            }
            _ => Event::Ignored,
        };
        match event {
            Event::Session { model_provider } => official = model_provider == "openai",
            Event::Turn { turn_id, model } => {
                models.insert(turn_id, model);
            }
            Event::Usage {
                thread_id,
                turn_id,
                response_id,
                usage,
            } => {
                if !official || thread_id != threadId || response_id.is_empty() {
                    return Err("逐请求事件的来源或归属不符合官方直连验收".into());
                }
                if [
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cached_input_tokens,
                    usage.reasoning_output_tokens,
                    usage.cache_write_input_tokens,
                ]
                .iter()
                .any(|count| *count < 0)
                    || usage.input_tokens.checked_add(usage.output_tokens)
                        != Some(usage.total_tokens)
                {
                    return Err("原生用量记录的计数不满足核对约束".into());
                }
                let model = models
                    .get(&turn_id)
                    .cloned()
                    .ok_or("逐请求事件缺少对应模型上下文")?;
                if requests
                    .insert(
                        response_id.clone(),
                        RequestUsage {
                            responseId: response_id,
                            model,
                            usage,
                        },
                    )
                    .is_some()
                {
                    return Err("同一请求出现重复的原生用量事件".into());
                }
            }
            Event::Ignored => {}
        }
    }
    if requests.is_empty() {
        return Err("本次会话没有逐请求用量记录".into());
    }
    Ok(requests.into_values().collect())
}

// 同一文件中切换模型的两轮请求应各自关联；正文事件只验证可跳过，不出现在返回结构中。
#[test]
fn parserAssociatesUsageWithItsTurn() {
    let fixture = concat!(
        "{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\"}}\n",
        "{\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"a\",\"model\":\"model-a\"}}\n",
        "{\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"b\",\"model\":\"model-b\"}}\n",
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"content\":\"ignored fixture text\"}}\n",
        "{\"type\":\"token_usage_record\",\"payload\":{\"thread_id\":\"fixture\",\"turn_id\":\"a\",\"response_id\":\"response-a\",\"usage\":{\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":3,\"total_tokens\":13,\"reasoning_output_tokens\":1,\"cache_write_input_tokens\":0}}}\n"
    );
    let parsed = parse(fixture.as_bytes(), "fixture").unwrap();
    assert_eq!(parsed[0].model, "model-a");
    assert_eq!(parsed[0].usage.total_tokens, 13);
    assert!(parse(fixture.as_bytes(), "other-thread").is_err());
}
