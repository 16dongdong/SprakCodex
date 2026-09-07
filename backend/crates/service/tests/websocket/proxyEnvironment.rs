//! 本机 WebSocket 夹具的代理环境事务；Windows 环境变量不区分大小写，清理顺序必须保留回环直连规则。
#![allow(non_snake_case, non_upper_case_globals)]
use super::EnvGuard;

const loopbackHosts: &str = "127.0.0.1,localhost,::1";
const proxyNames: [&str; 8] = [
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
];

// 原始值只由 EnvGuard 保存在内存，不实现 Debug，也不输出代理 URL 中可能存在的认证字段。
pub(super) struct LoopbackProxyEnvironment {
    guards: Vec<EnvGuard>,
}

impl LoopbackProxyEnvironment {
    // 调用方持有 test_env_guard；先清理各别名，最后发布 NO_PROXY，避免 Windows 的小写清理再次删除大写规则。
    pub(super) fn new() -> Self {
        let mut guards = Vec::with_capacity(proxyNames.len());
        for name in proxyNames.into_iter().take(proxyNames.len() - 1) {
            guards.push(EnvGuard::clear(name));
        }
        guards.push(EnvGuard::set("NO_PROXY", loopbackHosts));
        Self { guards }
    }
}

impl Drop for LoopbackProxyEnvironment {
    // Vec 默认正序析构会让后创建的同名别名覆盖原值；明确逆序回滚，正常结束与 panic 共用同一恢复顺序。
    fn drop(&mut self) {
        while self.guards.pop().is_some() {}
    }
}

// 回环绕行必须对大小写别名成立；退出后每个环境变量恢复原值，只做布尔比较而不在失败时打印秘密值。
#[test]
fn loopbackScopePreservesBypassAndRestoresEnvironment() {
    let _guard = crate::test_env_guard();
    let before: Vec<_> = proxyNames.iter().map(std::env::var_os).collect();
    {
        let _scope = LoopbackProxyEnvironment::new();
        assert!(std::env::var("NO_PROXY").as_deref() == Ok(loopbackHosts));
        #[cfg(windows)]
        assert!(std::env::var("no_proxy").as_deref() == Ok(loopbackHosts));
        for name in proxyNames.into_iter().take(6) {
            assert!(std::env::var_os(name).is_none());
        }
        // 在已知代理确实存在时检验绕行，避免因“根本没有代理”而得到没有证明力的 None。
        let _proxy = EnvGuard::set("ALL_PROXY", "http://127.0.0.1:9");
        assert!(
            tokio_tungstenite::tungstenite::proxy::ProxyConfig::from_env(
                &"ws://fixture.example:80".parse().unwrap()
            )
            .unwrap()
            .is_some()
        );
        for target in ["ws://127.0.0.1:80", "ws://localhost:80", "ws://[::1]:80"] {
            assert!(
                tokio_tungstenite::tungstenite::proxy::ProxyConfig::from_env(
                    &target.parse().unwrap()
                )
                .unwrap()
                .is_none()
            );
        }
    }
    assert!(proxyNames.iter().map(std::env::var_os).collect::<Vec<_>>() == before);
}

// 模拟断言异常退出仍应逆序恢复 Windows 的大小写别名，不污染后续测试或父进程继承的代理环境。
#[test]
fn panicRestoresEnvironmentAliases() {
    let _guard = crate::test_env_guard();
    let before: Vec<_> = proxyNames.iter().map(std::env::var_os).collect();
    let result = std::panic::catch_unwind(|| {
        let _scope = LoopbackProxyEnvironment::new();
        panic!("预期的环境事务回滚验收");
    });
    assert!(result.is_err());
    assert!(proxyNames.iter().map(std::env::var_os).collect::<Vec<_>>() == before);
}
