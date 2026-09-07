use super::*;
use std::{io::Write, path::PathBuf};

const context: &str = concat!(
    "{\"timestamp\":\"1970-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"fixture\",\"model_provider\":\"openai\"}}\n",
    "{\"timestamp\":\"1970-01-01T00:00:00Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"turn\",\"model\":\"gpt-5.4-mini\"}}\n"
);
const completed: &str = "{\"timestamp\":\"1970-01-01T00:00:02Z\",\"type\":\"token_usage_record\",\"payload\":{\"thread_id\":\"fixture\",\"turn_id\":\"turn\",\"response_id\":\"response\",\"usage\":{\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":3,\"total_tokens\":13,\"reasoning_output_tokens\":1,\"cache_write_input_tokens\":0}}}\n";

// 每例独占普通文件，不需要数据库或真实 CLI；析构严格删除该文件。
struct Fixture(PathBuf);
impl Fixture {
    // 唯一测试文件仅存静态协议样本。
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("clientEvent{:032x}.jsonl", rand::random::<u128>()));
        std::fs::write(&path, context).unwrap();
        Self(path)
    }
    // 模拟追加写入与半条 JSON，所有错误直接使测试失败。
    fn append(&self, content: &[u8]) {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&self.0)
            .unwrap()
            .write_all(content)
            .unwrap();
    }
}
impl Drop for Fixture {
    // 没有长期打开的文件句柄，删除失败代表解析器资源回收有误。
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

// 半条事件等待后续字节，历史上下文仍用于关联模型；再次轮询不得重新提交已确认记录。
#[test]
fn fragmentedEventRetainsContextWithoutReplaying() {
    let fixture = Fixture::new();
    let mut journal = Journal::default();
    fixture.append(&completed.as_bytes()[..100]);
    journal
        .poll(&fixture.0, 1000, &mut |_| panic!("半条完成事件不应提交"))
        .unwrap();
    fixture.append(&completed.as_bytes()[100..]);
    let mut records = Vec::new();
    journal
        .poll(&fixture.0, 1000, &mut |record| {
            records.push(record);
            Ok(())
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].parsed.model.as_deref(), Some("gpt-5.4-mini"));
    assert_eq!(records[0].parsed.usage.total_tokens, Some(13));
    assert!(records[0].pricingAllowed);
    journal
        .poll(&fixture.0, 1000, &mut |_| panic!("已提交事件不应重复读取"))
        .unwrap();
}

// 提交失败不推进该事件的位置，下一次可重试；旧于启用时刻的完成事件不被回填。
#[test]
fn failedCommitRetriesAndEarlierUsageIsExcluded() {
    let fixture = Fixture::new();
    fixture.append(completed.as_bytes());
    let mut journal = Journal::default();
    assert!(journal
        .poll(&fixture.0, 1000, &mut |_| Err("fixture".into()))
        .is_err());
    let mut count = 0;
    journal
        .poll(&fixture.0, 1000, &mut |_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 1);
    Journal::default()
        .poll(&fixture.0, 3000, &mut |_| panic!("历史用量不应回填"))
        .unwrap();
}

// 正文和未知 payload 不进入输出，非法计数明确失败而不是补零。
#[test]
fn ignoredBodyDoesNotAffectUsageAndBadCountersFail() {
    let fixture = Fixture::new();
    fixture.append(
        format!(
            "{{\"type\":\"response_item\",\"payload\":{{\"content\":\"{}\"}}}}\n",
            "ignored".repeat(10000)
        )
        .as_bytes(),
    );
    fixture.append(
        completed
            .replace("\"input_tokens\":10", "\"input_tokens\":-1")
            .as_bytes(),
    );
    assert!(Journal::default()
        .poll(&fixture.0, 0, &mut |_| panic!("非法用量不应提交"))
        .is_err());
}
