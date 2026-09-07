use super::*;

// 模拟第一次宿主提交失败和第二次宿主恢复，不使用成功计数代替磁盘文件的确认/保留行为。
#[test]
fn failureRetainsEventAndRestartCommitsBeforeCleanup() {
    let directory =
        std::env::temp_dir().join(format!("completionConsumer{:032x}", rand::random::<u128>()));
    std::fs::create_dir(&directory).unwrap();
    let record: completionSpool::Completion = serde_json::from_str(include_str!(
        "../../../directCommon/tests/fixtures/completion.json"
    ))
    .unwrap();
    completionSpool::publish(&directory, &record).unwrap();
    let path = directory.join(record.fileName());
    assert_eq!(
        commitFile(&path, |_| Err("模拟数据库未提交")),
        Err("模拟数据库未提交")
    );
    assert!(path.is_file());
    let mut observed = false;
    commitFile(&path, |restored| {
        assert_eq!(restored.responseId, record.responseId);
        assert_eq!(restored.totalTokens, 13);
        observed = true;
        Ok(())
    })
    .unwrap();
    assert!(observed && !path.exists());
    std::fs::remove_dir(directory).unwrap();
}
