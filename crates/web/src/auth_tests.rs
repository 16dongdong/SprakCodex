use super::*;

/// 函数 `login_force_requested_accepts_truthy_flags`
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
fn login_force_requested_accepts_truthy_flags() {
    for value in ["1", "true", "TRUE", "yes", "on"] {
        let query = LoginQuery {
            force: Some(value.to_string()),
        };
        assert!(login_force_requested(&query), "value={value}");
    }
    for value in ["", "0", "false", "no", "off"] {
        let query = LoginQuery {
            force: Some(value.to_string()),
        };
        assert!(!login_force_requested(&query), "value={value}");
    }
    assert!(!login_force_requested(&LoginQuery::default()));
}

/// 函数 `login_success_html_marks_current_tab_session`
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
fn login_success_html_marks_current_tab_session() {
    let html = login_success_html();
    assert!(html.contains("sessionStorage.setItem"));
    assert!(html.contains(WEB_AUTH_TAB_SESSION_STORAGE_KEY));
    assert!(html.contains("location.replace(\"/\")"));
}

#[test]
fn web_auth_allows_static_assets_without_session() {
    for path in [
        "/_next/static/chunks/app/page.js",
        "/_next/static/css/app.css",
        "/favicon.ico",
        "/author-alipay.jpg",
        "/manifest.json",
    ] {
        assert!(is_public_static_asset_path(path), "path={path}");
    }
    assert!(!is_public_static_asset_path("/settings"));
    assert!(!is_public_static_asset_path("/api/rpc"));
}
