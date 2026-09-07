use super::*;

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    /// 函数 `set`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - key: 参数 key
    /// - value: 参数 value
    ///
    /// # 返回
    /// 返回函数执行结果
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var_os(key);
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    /// 函数 `drop`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 无
    fn drop(&mut self) {
        if let Some(value) = self.previous.as_ref() {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

/// 函数 `ensure_default_db_path_resolves_relative_env_against_exe_dir`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn ensure_default_db_path_resolves_relative_env_against_exe_dir() {
    let _db_guard = EnvGuard::set(ENV_DB_PATH, Some("./data/codexmanager.db"));

    let resolved = ensure_default_db_path();

    assert_eq!(resolved, exe_dir().join("data").join("codexmanager.db"));
    assert_eq!(
        std::env::var(ENV_DB_PATH).ok().as_deref(),
        Some(resolved.to_string_lossy().as_ref())
    );
}

/// 函数 `rpc_token_file_path_resolves_relative_env_against_exe_dir`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn rpc_token_file_path_resolves_relative_env_against_exe_dir() {
    let _db_guard = EnvGuard::set(ENV_DB_PATH, Some("./data/codexmanager.db"));
    let _token_guard = EnvGuard::set(ENV_RPC_TOKEN_FILE, Some("./data/codexmanager.rpc-token"));

    let resolved = rpc_token_file_path();

    assert_eq!(
        resolved,
        exe_dir().join("data").join("codexmanager.rpc-token")
    );
}
