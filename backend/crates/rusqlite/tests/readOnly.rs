use rusqlite::Connection;
use std::time::{SystemTime,UNIX_EPOCH};

// 外部元数据连接必须在 SQLite 层拒绝写操作，且不会创建缺失数据库。
#[test]
fn external_database_is_read_only() {
    let root=std::env::temp_dir().join(format!("metadata-readonly-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&root).unwrap();
    let file=root.join("state.sqlite");
    {
        let writer=Connection::open(&file).unwrap();
        writer.execute_batch("CREATE TABLE threads(id TEXT,title TEXT,cwd TEXT); INSERT INTO threads VALUES('fixture','测试标题','fixture-project');").unwrap();
        let reader=Connection::openReadOnly(&file).unwrap();
        assert_eq!(reader.query_row("SELECT COUNT(*) FROM threads",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(reader.execute("DELETE FROM threads",[]).is_err());
        assert!(reader.execute("CREATE TABLE unwanted(value TEXT)",[]).is_err());
    }
    let missing=root.join("missing.sqlite");
    assert!(Connection::openReadOnly(&missing).is_err());
    assert!(!missing.exists());
    std::fs::remove_dir_all(root).unwrap();
}
