//! 完整报文顺序落入自动删除的临时文件；提交前脱敏，释放句柄即清理，不再截取固定长度预览。
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) struct Capture {
    file: Option<std::fs::File>,
    length: u64,
    failed: bool,
}

impl Default for Capture {
    // 临时文件创建失败仅标记采集失败，不阻止原始请求转发；不把缺失数据标为完整报文。
    fn default() -> Self {
        Self {
            file: None,
            length: 0,
            failed: false,
        }
    }
}

impl Capture {
    // 完整追加实际报文字节，不设预览截断；磁盘故障在详情中显式报告，认证材料不进入日志。
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || self.failed {
            return;
        }
        if self.file.is_none() {
            match tempfile::tempfile() {
                Ok(file) => self.file = Some(file),
                Err(_) => {
                    self.failed = true;
                    return;
                }
            }
        }
        match self.file.as_mut().ok_or(()).and_then(|file| {
            file.seek(SeekFrom::End(0))
                .and_then(|_| file.write_all(bytes))
                .map_err(|_| ())
        }) {
            Ok(()) => self.length += bytes.len() as u64,
            Err(()) => self.failed = true,
        }
    }

    // 完成时读取并脱敏全部内容；JSON 保持结构，SSE/文本保留帧顺序，二进制使用显式 Base64 表示。
    // 克隆句柄仅在采集锁内访问，后续追加总从 EOF 开始，不依赖共享游标位置。
    pub fn snapshot(&self) -> Value {
        self.snapshotEncoded(None)
    }

    // 请求压缩格式从原始头读取；Codex 的 zstd 请求解压后逐字段脱敏，不能把压缩凭据当作普通二进制保存。
    pub fn snapshotEncoded(&self, encoding: Option<&str>) -> Value {
        if !self.failed && self.length == 0 {
            return Value::String(String::new());
        }
        if self.failed {
            return json!({"captureError":"完整报文采集失败：临时存储读写异常"});
        }
        let content = (|| -> std::io::Result<Vec<u8>> {
            let mut file = self
                .file
                .as_ref()
                .ok_or_else(|| std::io::Error::other("缺少采集句柄"))?
                .try_clone()?;
            file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let Ok(mut bytes) = content else {
            return json!({"captureError":"完整报文读取失败"});
        };
        if encoding.is_some_and(|encoding| encoding.eq_ignore_ascii_case("zstd")) {
            bytes = match zstd::stream::decode_all(bytes.as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return json!({"captureError":"请求正文 zstd 解压失败"}),
            };
        }
        if bytes.is_empty() {
            return Value::String(String::new());
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            return redact(value);
        }
        match std::str::from_utf8(&bytes) {
            Ok(text) => Value::String(
                text.split_inclusive('\n')
                    .map(|line| {
                        let (prefix, content) = line
                            .strip_prefix("data: ")
                            .map(|content| ("data: ", content))
                            .or_else(|| {
                                line.strip_prefix("data:").map(|content| ("data:", content))
                            })
                            .unwrap_or(("", line));
                        if let Ok(value @ (Value::Object(_) | Value::Array(_))) =
                            serde_json::from_str::<Value>(content)
                        {
                            return format!(
                                "{prefix}{}{}",
                                redact(value),
                                if line.ends_with('\n') { "\n" } else { "" }
                            );
                        }
                        redact(Value::String(line.to_owned()))
                            .as_str()
                            .unwrap_or("")
                            .to_owned()
                    })
                    .collect(),
            ),
            Err(_) => {
                use base64::Engine;
                json!({"encoding":"base64","bytes":bytes.len(),"content":base64::engine::general_purpose::STANDARD.encode(&bytes)})
            }
        }
    }
}

// 凭据字段递归替换；文本中常见 Bearer、JWT 与 API key 也掩码，错误正文使用同一规则。
pub(super) fn redact(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| {
                    let name = key.to_ascii_lowercase().replace(['_', '-'], "");
                    let secret =
                        matches!(name.as_str(), "token" | "key" | "credential" | "session")
                            || [
                                "authorization",
                                "cookie",
                                "setcookie",
                                "accesstoken",
                                "refreshtoken",
                                "idtoken",
                                "sessiontoken",
                                "apikey",
                                "password",
                                "secret",
                                "privatekey",
                            ]
                            .iter()
                            .any(|field| name.contains(field));
                    (
                        key,
                        if secret {
                            Value::String("[已脱敏]".into())
                        } else {
                            redact(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact).collect()),
        Value::String(text) => {
            static secretPattern: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let pattern = secretPattern.get_or_init(|| regex::Regex::new(r"(?i)Bearer\s+[^\s\x22]+|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+|sk-[A-Za-z0-9_-]+|rt_[A-Za-z0-9_-]+").expect("静态凭据脱敏表达式"));
            Value::String(pattern.replace_all(&text, "[已脱敏]").into_owned())
        }
        value => value,
    }
}

// 只保存可读头值并应用统一掩码；调用方使用转发前的副本，不移除真实请求中的认证。
pub(super) fn headers(headers: &hyper::HeaderMap) -> Value {
    redact(Value::Object(
        headers
            .keys()
            .map(|name| {
                let values: Vec<Value> = headers
                    .get_all(name)
                    .iter()
                    .map(|value| match value.to_str() {
                    Ok(text) => Value::String(text.to_owned()),
                    Err(_) => { use base64::Engine; json!({"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(value.as_bytes())}) },
                })
                    .collect();
                (
                    name.to_string(),
                    if values.len() == 1 {
                        values.into_iter().next().unwrap()
                    } else {
                        Value::Array(values)
                    },
                )
            })
            .collect(),
    ))
}

#[cfg(test)]
#[path = "../../tests/observation/detailCaptureTests.rs"]
mod tests;
