/// 个人版服务无论接收旧的全接口模式还是远端主机地址，都只返回本机回环监听地址。
#[test]
fn personal_service_listener_is_loopback_only() {
    assert_eq!(
        codexmanager_service::listener_bind_addr_for_mode(
            "0.0.0.0:48760",
            codexmanager_service::SERVICE_BIND_MODE_ALL_INTERFACES,
        ),
        "localhost:48760"
    );
    assert_eq!(
        codexmanager_service::listener_bind_addr_for_mode(
            "192.0.2.1:49999",
            codexmanager_service::SERVICE_BIND_MODE_ALL_INTERFACES,
        ),
        "localhost:49999"
    );
}

/// 当前模式始终报告 loopback，旧环境变量和旧数据库值不再改变产品边界。
#[test]
fn personal_service_mode_is_fixed_to_loopback() {
    assert_eq!(
        codexmanager_service::current_service_bind_mode(),
        codexmanager_service::SERVICE_BIND_MODE_LOOPBACK
    );
    assert!(!codexmanager_service::bind_all_interfaces_enabled());
}
