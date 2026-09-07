//! 完成事件采用独立的原子文件事务；仅含计量元数据，不伪造客户端会话文件，也不携带认证或消息正文。
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const settingsName: &str = "capture.json";
pub const controlName: &str = "captureControl.json";
pub const directoryName: &str = "observationCompletions";
const maxSettingsBytes: u64 = 128 * 1024;
const maxRecordBytes: u64 = 4096;
static sequence: AtomicU64 = AtomicU64::new(0);

// 开关与 Relay 寿命分开：宿主正常退出保留启用选择，目标可把完成事件留待下次启动接收。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub directory: PathBuf,
}

// 模块旁只保存数据目录定位；真正开关在数据目录共享，升级后的多个已加载模块不能各自保留过期开关。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub directory: PathBuf,
}

// 固定协议版本与有界字段构成唯一元数据模型，客户端和宿主同时验证，不接受扩展正文或未知字段。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub version: u32,
    pub provider: String,
    pub model: String,
    pub threadId: String,
    pub turnId: String,
    pub responseId: String,
    pub timestampMillis: i64,
    pub inputTokens: i64,
    pub cachedInputTokens: i64,
    pub cacheWriteInputTokens: i64,
    pub outputTokens: i64,
    pub reasoningOutputTokens: i64,
    pub totalTokens: i64,
}

impl Completion {
    // 来源、计数与路径归属先验证；未知或溢出的计数绝不转换成零或参与费用估计。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.provider != "openai"
            || !text(&self.model)
            || !text(&self.turnId)
            || !text(&self.responseId)
            || !self.responseId.starts_with("resp_")
            || !uuid(&self.threadId)
            || self.timestampMillis <= 0
        {
            return Err("完成事件来源或标识无效");
        }
        if [
            self.inputTokens,
            self.cachedInputTokens,
            self.cacheWriteInputTokens,
            self.outputTokens,
            self.reasoningOutputTokens,
            self.totalTokens,
        ]
        .iter()
        .any(|count| *count < 0)
            || self.cachedInputTokens > self.inputTokens
            || self.reasoningOutputTokens > self.outputTokens
            || self.inputTokens.checked_add(self.outputTokens) != Some(self.totalTokens)
        {
            return Err("完成事件计数无效");
        }
        Ok(())
    }

    // 响应标识只进入散列，线程 UUID 经过严格校验；上游字符串不直接作为任意路径组件。
    pub fn fileName(&self) -> String {
        format!(
            "runtime-{:x}-{}.jsonl",
            Sha256::digest(self.responseId.as_bytes()),
            self.threadId
        )
    }
}

// 文本仅允许有界可打印元数据；控制字符和空值不进入文件或日志。
fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

// UUID 的每一位都按位置检查，禁止分隔符、路径字符或宽松字符串格式参与文件命名。
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

// 只读有界普通文件；符号链接及过大内容在反序列化前拒绝，错误不输出文件内容。
fn readBounded(path: &Path, limit: u64) -> Result<Vec<u8>, &'static str> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "读取完成事件文件属性失败")?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("完成事件文件类型或大小无效");
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "打开完成事件文件失败")?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取完成事件文件失败")?;
    if bytes.len() as u64 > limit {
        return Err("完成事件文件超过限额");
    }
    Ok(bytes)
}

// 未部署控制文件或显式停用都不捕获；损坏配置返回错误，不猜测另一个目录或沿用旧配置。
pub fn activeDirectory(module: &Path) -> Result<Option<PathBuf>, &'static str> {
    let path = module.parent().ok_or("模块目录缺失")?.join(settingsName);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("读取完成事件控制状态失败"),
        Ok(_) => {}
    }
    let location: Location = serde_json::from_slice(&readBounded(&path, maxSettingsBytes)?)
        .map_err(|_| "完成事件定位格式无效")?;
    if !location.directory.is_absolute() {
        return Err("完成事件目录不是绝对路径");
    }
    let settings: Settings = serde_json::from_slice(&readBounded(
        &location.directory.join(controlName),
        maxSettingsBytes,
    )?)
    .map_err(|_| "完成事件控制格式无效")?;
    if settings.directory != location.directory {
        return Err("完成事件控制目录不匹配");
    }
    Ok(settings.enabled.then_some(settings.directory))
}

// 一次完成只写一个有界文件，fsync 和关闭后原子发布；宿主只读取最终文件，不观察半条 JSON。
pub fn publish(directory: &Path, completion: &Completion) -> Result<(), &'static str> {
    completion.validate()?;
    if !directory.is_absolute() {
        return Err("完成事件目录不是绝对路径");
    }
    let bytes = serde_json::to_vec(completion).map_err(|_| "完成事件编码失败")?;
    if bytes.len() as u64 > maxRecordBytes {
        return Err("完成事件编码超过限额");
    }
    let finalPath = directory.join(completion.fileName());
    let stage = directory.join(format!(
        ".{}-{}-{}.pending",
        completion.fileName(),
        std::process::id(),
        sequence.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .map_err(|_| "创建完成事件临时文件失败")?;
    let written = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = written.and_then(|_| std::fs::rename(&stage, &finalPath));
    if result.is_err() {
        std::fs::remove_file(&stage).map_err(|_| "完成事件失败且临时文件清理失败")?;
        return Err("完成事件原子发布失败");
    }
    Ok(())
}

// 消费者对内容和文件名交叉核对；不接受把另一个响应或线程的内容放进当前文件名。
pub fn read(path: &Path) -> Result<Completion, &'static str> {
    let completion: Completion = serde_json::from_slice(&readBounded(path, maxRecordBytes)?)
        .map_err(|_| "完成事件格式无效")?;
    completion.validate()?;
    if path.file_name().and_then(|name| name.to_str()) != Some(completion.fileName().as_str()) {
        return Err("完成事件文件归属不匹配");
    }
    Ok(completion)
}

// 扫描只选择本协议已发布文件，临时副本永不进入数据库消费者。
pub fn isReady(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("runtime-") && name.ends_with(".jsonl"))
}

#[cfg(test)]
#[path = "../tests/unit/completionSpoolTests.rs"]
mod tests;
