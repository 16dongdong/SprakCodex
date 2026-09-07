use super::*;

// 每个用例创建独占目录；文件都是本测试写入，Drop 不递归删除外部路径。
struct Fixture(PathBuf);
impl Fixture {
    // 单调序号与进程标识隔离并发测试，时间部分避免跨次运行复用旧目录。
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "completionSpool-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            sequence.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Fixture {
    // 测试不留下待发布副本；意外目录必须使断言失败，禁止越界清理。
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::remove_file(entry.path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

// 跨端共用同一脱敏 golden fixture，避免测试中的字段名或计量单位独立漂移。
fn completion() -> Completion {
    serde_json::from_str(include_str!("../fixtures/completion.json")).unwrap()
}

// 完整写入后才能被消费者选择；重复响应使用相同最终路径，没有临时副本或重复队列条目。
#[test]
fn atomicPublishRoundTripsWithoutPendingFiles() {
    let fixture = Fixture::new();
    let record = completion();
    publish(&fixture.0, &record).unwrap();
    publish(&fixture.0, &record).unwrap();
    let path = fixture.0.join(record.fileName());
    assert!(isReady(&path));
    let restored = read(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&record).unwrap()
    );
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
}

// 非法来源、路径标识与溢出计数在文件创建前拒绝。
#[test]
fn invalidCompletionNeverCreatesAFile() {
    let fixture = Fixture::new();
    let mut record = completion();
    record.provider = "custom".into();
    assert!(publish(&fixture.0, &record).is_err());
    record = completion();
    record.threadId = "../outside".into();
    assert!(publish(&fixture.0, &record).is_err());
    record = completion();
    record.inputTokens = i64::MAX;
    assert!(publish(&fixture.0, &record).is_err());
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 0);
}

// 同一数据目录的所有模块定位都读取同一个开关，不能在模块升级后各自保留旧的启用副本。
#[test]
fn locatorUsesSharedControlAndRejectsCorruption() {
    let fixture = Fixture::new();
    let module = fixture.0.join("cphook.dll");
    assert!(activeDirectory(&module).unwrap().is_none());
    let location = Location {
        directory: fixture.0.clone(),
    };
    std::fs::write(
        fixture.0.join(settingsName),
        serde_json::to_vec(&location).unwrap(),
    )
    .unwrap();
    for enabled in [true, false, true] {
        std::fs::write(
            fixture.0.join(controlName),
            serde_json::to_vec(&Settings {
                enabled,
                directory: fixture.0.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(activeDirectory(&module).unwrap().is_some(), enabled);
    }
    std::fs::write(fixture.0.join(controlName), b"broken").unwrap();
    assert!(activeDirectory(&module).is_err());
}

// 发布失败只清理本次临时文件，不删除已有目标；错误和未知字段均不把内容误当合法元数据。
#[test]
fn failedPublishCleansItsStageAndReadRejectsWrongIdentity() {
    let fixture = Fixture::new();
    let record = completion();
    let destination = fixture.0.join(record.fileName());
    std::fs::create_dir(&destination).unwrap();
    assert!(publish(&fixture.0, &record).is_err());
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
    std::fs::remove_dir(&destination).unwrap();
    publish(&fixture.0, &record).unwrap();
    let moved = fixture.0.join("runtime-wrong.jsonl");
    std::fs::rename(&destination, &moved).unwrap();
    assert!(read(&moved).is_err());
    let mut encoded = serde_json::to_value(&record).unwrap();
    encoded["unexpected"] = serde_json::json!(true);
    std::fs::write(&destination, serde_json::to_vec(&encoded).unwrap()).unwrap();
    assert!(read(&destination).is_err());
}
