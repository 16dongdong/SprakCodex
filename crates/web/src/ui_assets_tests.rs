use super::*;

/// 函数 `spa_route_fallback_uses_html_content_type`
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
fn spa_route_fallback_uses_html_content_type() {
    let response = serve_embedded_path("accounts");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html")
    );
}

/// 函数 `directory_route_prefers_embedded_directory_index`
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
fn html_document_uses_no_store_cache_headers() {
    let response = serve_embedded_path("settings");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store, no-cache, must-revalidate")
    );
    assert_eq!(
        response
            .headers()
            .get(header::PRAGMA)
            .and_then(|value| value.to_str().ok()),
        Some("no-cache")
    );
    assert_eq!(
        response
            .headers()
            .get(header::EXPIRES)
            .and_then(|value| value.to_str().ok()),
        Some("0")
    );
}

#[test]
fn static_asset_does_not_get_no_store_cache_headers() {
    let response = serve_embedded_path("favicon.ico");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::CACHE_CONTROL).is_none());
}

#[test]
fn directory_route_prefers_embedded_directory_index() {
    let (served_path, _) = resolve_embedded_asset("accounts/").expect("accounts asset");
    assert_eq!(served_path, "accounts/index.html");

    let (served_path, _) = resolve_embedded_asset("accounts").expect("accounts asset");
    assert_eq!(served_path, "accounts/index.html");
}

#[test]
fn missing_embedded_asset_does_not_fallback_to_index_html() {
    assert!(resolve_embedded_asset("_next/static/chunks/missing-test.js").is_none());
    assert!(resolve_embedded_asset("missing-image.png").is_none());
}
