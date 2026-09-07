use super::{plugin_http_client, plugin_http_client_build_count_for_test};

#[test]
fn plugin_http_client_reuses_cached_client() {
    let before = plugin_http_client_build_count_for_test();
    let first = plugin_http_client().expect("first plugin http client");
    let after_first = plugin_http_client_build_count_for_test();
    let second = plugin_http_client().expect("second plugin http client");
    let after_second = plugin_http_client_build_count_for_test();

    assert!(
        after_first == before || after_first == before + 1,
        "expected first call to reuse an existing client or build one client, before={before}, after_first={after_first}"
    );
    assert_eq!(after_second, after_first);
    drop(first);
    drop(second);
}
