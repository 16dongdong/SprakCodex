use codexmanager_core::storage::Storage;
use std::{path::PathBuf, sync::{Mutex, OnceLock}, time::{Duration, Instant}};

static LAST_SYNC: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

// 最多每十五秒同步一次，只读取已接入会话对应的标题、名称和工作目录；错误作为页面诊断返回。
pub(super) fn sync(storage: &Storage) -> Result<(), String> {
    let mut last = LAST_SYNC.get_or_init(||Mutex::new(None)).lock().map_err(|_|"会话元数据同步锁损坏")?;
    if last.is_some_and(|time|time.elapsed()<Duration::from_secs(15)) {return Ok(());}
    let home = std::env::var_os("CODEX_HOME").filter(|value|!value.is_empty()).map(PathBuf::from).or_else(||std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).map(|home|PathBuf::from(home).join(".codex"))).ok_or("未找到客户端目录")?;
    if !home.is_absolute() {return Err("客户端目录必须为绝对路径".into());}
    let mut files=Vec::new();
    for entry in std::fs::read_dir(&home).map_err(|e|format!("读取客户端目录失败：{e}"))? {
        let path=entry.map_err(|e|e.to_string())?.path();
        if path.extension().and_then(|v|v.to_str())!=Some("sqlite") {continue;}
        if let Some(version)=path.file_stem().and_then(|v|v.to_str()).and_then(|v|v.strip_prefix("state_")).and_then(|v|v.parse::<u32>().ok()) {files.push((version,path));}
    }
    let (_,path)=files.into_iter().max_by_key(|(version,_)|*version).ok_or("未找到客户端会话元数据库")?;
    let connection=rusqlite::Connection::openReadOnly(path).map_err(|e|format!("只读打开会话元数据失败：{e}"))?;
    let fields=connection.prepare("PRAGMA table_info(threads)").map_err(|e|e.to_string())?.query_map([],|row|row.get::<_,String>(1)).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
    if !["id","title","cwd"].iter().all(|field|fields.iter().any(|f|f==field)) {return Err("客户端会话元数据结构不受支持".into());}
    let title=if fields.iter().any(|f|f=="name") {"COALESCE(NULLIF(name,''),title)"} else {"title"};
    let ids=storage.routingSessionIds().map_err(|e|e.to_string())?;
    let mut display=Vec::new();
    for chunk in ids.chunks(400) {
        let placeholders=vec!["?";chunk.len()].join(",");
        let sql=format!("SELECT id,{title},cwd FROM threads WHERE id IN ({placeholders})");
        let mut query=connection.prepare(&sql).map_err(|e|e.to_string())?;
        let rows=query.query_map(rusqlite::params_from_iter(chunk.iter()),|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?;
        for row in rows {display.push(row.map_err(|e|e.to_string())?);}
    }
    storage.syncRoutingDisplay(&display).map_err(|e|e.to_string())?;
    *last=Some(Instant::now());
    Ok(())
}
