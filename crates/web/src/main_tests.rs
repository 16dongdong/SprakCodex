use super::*;
use axum::body::to_bytes;

/// 函数 `web_auth_cookie_is_scoped_by_process_session_key`
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
fn web_auth_cookie_is_scoped_by_process_session_key() {
    let password_hash = "sha256$abc$def";
    let rpc_token = "rpc-token";

    let first = auth::build_web_auth_cookie_value(password_hash, rpc_token, "session-a");
    let second = auth::build_web_auth_cookie_value(password_hash, rpc_token, "session-b");

    assert_ne!(first, second);
}

/// 函数 `parse_cookie_value_returns_matching_cookie`
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
fn parse_cookie_value_returns_matching_cookie() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("a=1; codexmanager_web_auth=token-123; b=2"),
    );

    let actual = auth::parse_cookie_value(&headers, WEB_AUTH_COOKIE_NAME);

    assert_eq!(actual.as_deref(), Some("token-123"));
}

/// 函数 `normalize_connect_addr_maps_all_interfaces_to_localhost`
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
fn normalize_connect_addr_maps_all_interfaces_to_localhost() {
    assert_eq!(
        normalize_connect_addr("0.0.0.0:48760").as_deref(),
        Some("localhost:48760")
    );
    assert_eq!(
        normalize_connect_addr("[::]:48760").as_deref(),
        Some("localhost:48760")
    );
    assert_eq!(
        normalize_connect_addr("192.168.1.8:48760").as_deref(),
        Some("192.168.1.8:48760")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn parse_linux_default_gateway_ipv4_reads_default_route() {
    let routes = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\neth0\t00000000\t010011AC\t0003\t0\t0\t0\t00000000\t0\t0\t0\n";
    assert_eq!(
        parse_linux_default_gateway_ipv4(routes),
        Some(Ipv4Addr::new(172, 17, 0, 1))
    );
}

/// 函数 `browser_open_addr_maps_all_interfaces_to_loopback`
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
fn browser_open_addr_maps_all_interfaces_to_loopback() {
    assert_eq!(
        browser_open_addr("0.0.0.0:48761").as_deref(),
        Some("127.0.0.1:48761")
    );
    assert_eq!(
        browser_open_addr("[::]:48761").as_deref(),
        Some("127.0.0.1:48761")
    );
    assert_eq!(
        browser_open_addr("192.168.1.8:48761").as_deref(),
        Some("192.168.1.8:48761")
    );
}

/// 函数 `runtime_info_reports_web_gateway_capabilities`
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
#[tokio::test]
async fn runtime_info_reports_web_gateway_capabilities() {
    let response = runtime_info().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read runtime response body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("parse runtime response json");

    assert_eq!(payload["mode"], "web-gateway");
    assert_eq!(payload["rpcBaseUrl"], "/api/rpc");
    assert_eq!(
        payload["authorContentUrl"],
        "https://author.qxnm.top/api/public/author-content"
    );
    assert_eq!(payload["canManageService"], false);
    assert_eq!(payload["canSelfUpdate"], false);
    assert_eq!(payload["canCloseToTray"], false);
    assert_eq!(payload["canOpenLocalDir"], false);
    assert_eq!(payload["canUseBrowserFileImport"], true);
    assert_eq!(payload["canUseBrowserDownloadExport"], true);
}
