use super::*;

// 普通 HTTP URL、localhost 和 IP 字面量都需同时匹配端口；不做外部 DNS 解析或端口扫描。
#[test]
fn proxyIdentityIncludesAddressAndPort() {
    let mut endpoints = ProxyEndpoints::default();
    endpoints.addUrl("http://127.0.0.1:32123", "http");
    assert!(endpoints.matches("127.0.0.1:32123".parse().unwrap()));
    assert!(!endpoints.matches("127.0.0.2:32123".parse().unwrap()));
    assert!(!endpoints.matches("127.0.0.1:32124".parse().unwrap()));
    assert!(!endpoints.matches("[::1]:32123".parse().unwrap()));
    endpoints.addUrl("http://LOCALHOST:32124", "http");
    assert!(endpoints.matches("[::1]:32124".parse().unwrap()));
    assert!(endpoints.matches("127.0.0.1:32124".parse().unwrap()));
}

// https= 是目标请求类型，HTTPS 指令和 https:// 才是 TLS 到代理；两者必须区分。
#[test]
fn windowsProxySelectorsPreserveTransportMeaning() {
    let mut endpoints = ProxyEndpoints::default();
    endpoints.addSystemList(
        "http=127.0.0.1:32121;https=[::1]:32122;PROXY localhost:32123;HTTPS localhost:32124;DIRECT",
    );
    for address in ["127.0.0.1:32121", "[::1]:32122", "127.0.0.1:32123"] {
        assert!(endpoints.matches(address.parse().unwrap()));
    }
    assert!(!endpoints.matches("127.0.0.1:32124".parse().unwrap()));
}

// 本地认证代理、SOCKS 和 TLS 代理都不接受明文接入，同端点出现多协议配置时也不能猜测。
#[test]
fn unsupportedOrAmbiguousProxyRemainsOriginal() {
    for configured in [
        "https://127.0.0.1:32123",
        "socks5://127.0.0.1:32123",
        "http://fixture:fixture@127.0.0.1:32123",
    ] {
        let mut endpoints = ProxyEndpoints::default();
        endpoints.addUrl("http://127.0.0.1:32123", "http");
        endpoints.addUrl(configured, "http");
        assert!(!endpoints.matches("127.0.0.1:32123".parse().unwrap()));
    }
    let mut endpoints = ProxyEndpoints::default();
    endpoints.addSystemList("http=localhost:32123;socks=localhost:32123");
    assert!(!endpoints.matches("[::1]:32123".parse().unwrap()));
}

// 未知选择器、远端地址、零端口及残缺指令不产生本地候选；每次快照独立，删除设置后不保留旧端口。
#[test]
fn malformedAndRemovedSettingsDoNotCaptureLocalServices() {
    let mut endpoints = ProxyEndpoints::default();
    for value in [
        "unknown=127.0.0.1:32123",
        "INVALID 127.0.0.1:32123",
        "SOCKS 127.0.0.1:32123",
        "http://203.0.113.1:32123",
        "http://localhost:0",
        "http://[::1",
    ] {
        endpoints.addSystemList(value);
    }
    assert!(!endpoints.matches("127.0.0.1:32123".parse().unwrap()));
    endpoints.addSystemList("127.0.0.1:32124");
    assert!(endpoints.matches("127.0.0.1:32124".parse().unwrap()));
    assert!(!ProxyEndpoints::default().matches("127.0.0.1:32124".parse().unwrap()));
}
