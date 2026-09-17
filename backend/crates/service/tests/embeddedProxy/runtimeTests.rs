use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct EnvGuard {
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn setDatabase(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("CODEXMANAGER_DB_PATH");
        std::env::set_var("CODEXMANAGER_DB_PATH", path);
        Self { previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("CODEXMANAGER_DB_PATH", value),
            None => std::env::remove_var("CODEXMANAGER_DB_PATH"),
        }
    }
}

fn databasePath() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("embedded-proxy-{unique}.db"))
}

// 关闭状态也必须完整保存节点、订阅、代理组和链式字段，后续开启不重新解析用户配置。
#[test]
fn disabledConfigurationRoundTripsAllStructures() {
    let _testGuard = crate::test_env_guard();
    let path = databasePath();
    let _env = EnvGuard::setDatabase(&path);
    let mut extra = serde_json::Map::new();
    extra.insert("password".to_string(), serde_json::json!("secret"));
    let config = ProxyConfig {
        enabled: false,
        active: Some("group-a".to_string()),
        nodes: vec![config::ProxyNode {
            name: "node-a".to_string(),
            kind: "trojan".to_string(),
            server: "example.com".to_string(),
            port: 443,
            chain_entry: Some("node-b".to_string()),
            sub: Some("subscription-a".to_string()),
            extra,
        }],
        groups: vec![config::ProxyGroup {
            name: "group-a".to_string(),
            kind: "select".to_string(),
            members: vec!["node-a".to_string()],
            selected: Some("node-a".to_string()),
        }],
        subscriptions: vec![config::Subscription {
            name: "subscription-a".to_string(),
            url: "https://example.com/sub".to_string(),
            updated_at: 1,
            node_count: 1,
        }],
        mitm: false,
    };

    saveConfig(config.clone()).unwrap();
    assert_eq!(loadConfig().unwrap(), config);
    shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}
