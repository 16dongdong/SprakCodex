//! 正文详情独立读取，管理员或记录所属密钥的用户才可访问；历史未采集记录返回 None。
use super::Storage;
use rusqlite::{OptionalExtension, Result};

#[allow(non_snake_case)]
impl Storage {
    // 授权在查询内完成，None 表示管理员，Some 是调用者拥有的密钥集合；缺记录与越权返回相同结果。
    pub fn readRequestDetails(
        &self,
        trace: &str,
        keys: Option<&[String]>,
    ) -> Result<Option<String>> {
        let row: Option<(Option<String>, Option<String>)> = self.conn.query_row("SELECT r.key_id,d.payload FROM request_logs r LEFT JOIN request_details d ON d.request_log_id=r.id WHERE r.trace_id=?1", [trace], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        let Some((key, payload)) = row else {
            return Ok(None);
        };
        if let Some(keys) = keys {
            if !key.as_ref().is_some_and(|key| keys.contains(key)) {
                return Ok(None);
            }
        }
        Ok(payload)
    }
}

#[cfg(test)]
#[path = "tests/requestDetailsTests.rs"]
mod tests;
