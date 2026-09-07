use super::*;
use crate::storage::{
    Account, AccountProxyUrlTestInsertInput, ProxyDiagnosticTestInsertInput,
    ProxyProfileUrlTestInsertInput, ProxySpeedTestInsertInput,
};

fn account(id: &str) -> Account {
    Account {
        id: id.to_string(),
        label: id.to_string(),
        issuer: "chatgpt".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: 1,
        updated_at: 1,
    }
}

fn profile_update(id: &str) -> ProxyProfileUpdateInput {
    ProxyProfileUpdateInput {
        id: id.to_string(),
        name: None,
        proxy_url: None,
        enabled: None,
        status: None,
        last_error: None,
        last_url_latency_ms: None,
        last_download_mbps: None,
        last_upload_mbps: None,
        last_tested_at: None,
        ip: None,
        country_code: None,
        country_name: None,
        region_name: None,
        city_name: None,
        asn: None,
        as_org: None,
        isp: None,
        as_domain: None,
        flag_img_url: None,
        flag_emoji: None,
        timezone_id: None,
        timezone_offset: None,
        timezone_utc: None,
        tags_json: None,
        notes: None,
    }
}

fn create_profile(storage: &Storage, id: &str) -> ProxyProfile {
    storage
        .create_proxy_profile(&ProxyProfileCreateInput {
            id: id.to_string(),
            name: "Primary proxy".to_string(),
            proxy_url: "socks5h://user:secret@proxy.example:1080".to_string(),
            enabled: true,
            tags_json: Some(r#"["primary"]"#.to_string()),
            notes: Some("test profile".to_string()),
        })
        .expect("create proxy profile")
}

#[test]
fn proxy_profile_crud_preserves_secret_and_redacts_list_output() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let created = create_profile(&storage, "proxy-1");
    assert_eq!(
        created.proxy_url,
        "socks5h://user:secret@proxy.example:1080"
    );
    assert_eq!(created.proxy_url_redacted, "socks5h://proxy.example:1080");
    assert_eq!(created.scheme.as_deref(), Some("socks5h"));
    assert_eq!(created.host.as_deref(), Some("proxy.example"));
    assert_eq!(created.port, Some(1080));

    let listed = storage.list_proxy_profiles().expect("list proxy profiles");
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].proxy_url_redacted.contains("secret"));
    assert!(!listed[0].proxy_url_redacted.contains("user"));

    let mut update = profile_update("proxy-1");
    update.name = Some("Updated proxy".to_string());
    update.proxy_url = Some("http://next-user:next-secret@edge.example:8080".to_string());
    update.enabled = Some(false);
    update.status = Some("healthy".to_string());
    let updated = storage
        .update_proxy_profile(&update)
        .expect("update proxy profile")
        .expect("updated proxy profile");
    assert_eq!(updated.name, "Updated proxy");
    assert!(!updated.enabled);
    assert_eq!(updated.proxy_url_redacted, "http://edge.example:8080");
    assert_eq!(updated.host.as_deref(), Some("edge.example"));
    assert_eq!(updated.port, Some(8080));

    assert!(storage
        .delete_proxy_profile("proxy-1")
        .expect("delete proxy profile"));
    assert!(storage
        .find_proxy_profile("proxy-1")
        .expect("find deleted profile")
        .is_none());
}

#[test]
fn proxy_foreign_keys_clear_bindings_and_cascade_history() {
    let mut storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    storage
        .insert_account(&account("account-1"))
        .expect("insert account");
    create_profile(&storage, "proxy-1");

    storage
        .upsert_account_proxy_settings(
            "account-1",
            true,
            Some("profile"),
            Some("proxy-1"),
            None,
            "healthy",
            Some(12),
            Some(100),
            None,
            Some("203.0.113.10"),
            Some("US"),
            Some("United States"),
            None,
            None,
            Some(100),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("upsert account proxy settings");
    let settings = storage
        .find_account_proxy_settings("account-1")
        .expect("find account proxy settings")
        .expect("account proxy settings");
    assert!(settings.enabled);
    assert_eq!(settings.proxy_source.as_deref(), Some("profile"));
    assert_eq!(settings.proxy_profile_id.as_deref(), Some("proxy-1"));

    let url_test = storage
        .insert_proxy_profile_url_test(&ProxyProfileUrlTestInsertInput {
            proxy_profile_id: "proxy-1".to_string(),
            status: "healthy".to_string(),
            url_latency_ms: Some(12),
            status_code: Some(204),
            test_url: "https://example.com/health".to_string(),
            final_url: None,
            redirected: false,
            tested_at: 100,
            error_code: None,
            error: None,
        })
        .expect("insert profile url test");
    let profile_speed = storage
        .insert_proxy_speed_test(&ProxySpeedTestInsertInput {
            scope: "system_proxy".to_string(),
            proxy_profile_id: Some("proxy-1".to_string()),
            account_id: None,
            status: "healthy".to_string(),
            provider: "cloudflare".to_string(),
            observed_ip: None,
            observed_country: None,
            observed_colo: None,
            max_payload_bytes: Some(1024),
            samples_json: None,
            download_summary_json: None,
            upload_summary_json: None,
            started_at: 100,
            finished_at: 101,
            error_code: None,
            error: None,
        })
        .expect("insert profile speed test");
    let profile_diagnostic = storage
        .insert_proxy_diagnostic_test(&ProxyDiagnosticTestInsertInput {
            scope: "system_proxy".to_string(),
            proxy_profile_id: Some("proxy-1".to_string()),
            account_id: None,
            status: "healthy".to_string(),
            provider: "cloudflare".to_string(),
            file_size_id: "1mb".to_string(),
            downloaded_bytes: Some(1024),
            duration_ms: Some(10),
            mbps: Some(1.0),
            tested_at: 100,
            error: None,
        })
        .expect("insert profile diagnostic test");

    assert!(storage
        .delete_proxy_profile("proxy-1")
        .expect("delete proxy profile"));
    let settings = storage
        .find_account_proxy_settings("account-1")
        .expect("find settings after profile delete")
        .expect("settings remain after profile delete");
    assert_eq!(settings.proxy_profile_id, None);
    assert!(storage
        .find_proxy_profile_url_test(url_test.id)
        .expect("find deleted url test")
        .is_none());
    assert!(storage
        .find_proxy_speed_test(profile_speed.id)
        .expect("find deleted speed test")
        .is_none());
    assert!(storage
        .find_proxy_diagnostic_test(profile_diagnostic.id)
        .expect("find deleted diagnostic test")
        .is_none());

    let account_url_test = storage
        .insert_account_proxy_url_test(&AccountProxyUrlTestInsertInput {
            account_id: "account-1".to_string(),
            status: "healthy".to_string(),
            url_latency_ms: Some(10),
            status_code: Some(204),
            test_url: "https://example.com/health".to_string(),
            final_url: None,
            redirected: false,
            tested_at: 101,
            error_code: None,
            error: None,
        })
        .expect("insert account url test");
    let account_speed = storage
        .insert_proxy_speed_test(&ProxySpeedTestInsertInput {
            scope: "account_proxy".to_string(),
            proxy_profile_id: None,
            account_id: Some("account-1".to_string()),
            status: "healthy".to_string(),
            provider: "cloudflare".to_string(),
            observed_ip: None,
            observed_country: None,
            observed_colo: None,
            max_payload_bytes: Some(1024),
            samples_json: None,
            download_summary_json: None,
            upload_summary_json: None,
            started_at: 101,
            finished_at: 102,
            error_code: None,
            error: None,
        })
        .expect("insert account speed test");
    let account_diagnostic = storage
        .insert_proxy_diagnostic_test(&ProxyDiagnosticTestInsertInput {
            scope: "account_proxy".to_string(),
            proxy_profile_id: None,
            account_id: Some("account-1".to_string()),
            status: "healthy".to_string(),
            provider: "cloudflare".to_string(),
            file_size_id: "1mb".to_string(),
            downloaded_bytes: Some(1024),
            duration_ms: Some(10),
            mbps: Some(1.0),
            tested_at: 101,
            error: None,
        })
        .expect("insert account diagnostic test");

    storage.delete_account("account-1").expect("delete account");
    assert!(storage
        .find_account_proxy_settings("account-1")
        .expect("find deleted account settings")
        .is_none());
    assert!(storage
        .find_account_proxy_url_test(account_url_test.id)
        .expect("find deleted account url test")
        .is_none());
    assert!(storage
        .find_proxy_speed_test(account_speed.id)
        .expect("find deleted account speed test")
        .is_none());
    assert!(storage
        .find_proxy_diagnostic_test(account_diagnostic.id)
        .expect("find deleted account diagnostic test")
        .is_none());
}

#[test]
fn proxy_migrations_use_current_versions_and_pass_foreign_key_check() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    for version in [
        "117_account_proxy_settings",
        "118_proxy_profiles",
        "119_proxy_profile_url_tests",
        "120_proxy_history",
    ] {
        let applied: i64 = storage
            .conn
            .query_row(
                "SELECT COUNT(1) FROM schema_migrations WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .expect("query migration");
        assert_eq!(applied, 1, "migration {version} should be applied once");
    }

    let violations: i64 = storage
        .conn
        .query_row("SELECT COUNT(1) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key check");
    assert_eq!(violations, 0);
}
