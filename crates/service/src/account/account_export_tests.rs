use super::{
    build_account_export_payload, build_single_export_bundle_json, load_export_metadata,
    normalize_selected_account_ids, sanitize_file_stem,
};
use codexmanager_core::storage::{now_ts, Account, AccountAgentIdentity, Storage, Token};
use std::collections::HashMap;

fn sample_account(id: &str, label: &str) -> Account {
    Account {
        id: id.to_string(),
        label: label.to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: None,
        workspace_id: None,
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
    }
}

fn sample_token(account_id: &str) -> Token {
    Token {
        account_id: account_id.to_string(),
        id_token: "id".to_string(),
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        api_key_access_token: None,
        last_refresh: 0,
    }
}

/// 函数 `sanitize_file_stem_replaces_windows_invalid_chars`
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
fn sanitize_file_stem_replaces_windows_invalid_chars() {
    let actual = sanitize_file_stem(r#"a<b>c:d"e/f\g|h?i*j"#);
    assert_eq!(actual, "a_b_c_d_e_f_g_h_i_j");
}

/// 函数 `sanitize_file_stem_trims_tailing_space_and_dot`
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
fn sanitize_file_stem_trims_tailing_space_and_dot() {
    let actual = sanitize_file_stem(" demo. ");
    assert_eq!(actual, "demo");
}

/// 函数 `normalize_selected_account_ids_trims_deduplicates_and_sorts`
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
fn normalize_selected_account_ids_trims_deduplicates_and_sorts() {
    let selected = vec![
        " acc-2 ".to_string(),
        "".to_string(),
        "acc-1".to_string(),
        "acc-2".to_string(),
    ];
    let actual = normalize_selected_account_ids(&selected);

    assert_eq!(actual, vec!["acc-1".to_string(), "acc-2".to_string()]);
}

#[test]
fn load_export_metadata_reads_only_export_accounts() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    storage
        .insert_account(&sample_account("acc-exported", "exported account"))
        .expect("insert exported account");
    storage
        .insert_account(&sample_account("acc-ignored", "ignored account"))
        .expect("insert ignored account");
    storage
        .upsert_account_metadata("acc-exported", Some("exported note"), Some("tag-a"))
        .expect("insert exported metadata");
    storage
        .upsert_account_metadata("acc-ignored", Some("ignored note"), Some("tag-b"))
        .expect("insert ignored metadata");

    let metadata = load_export_metadata(
        &storage,
        &[sample_account("acc-exported", "exported account")],
    )
    .expect("load metadata");

    assert_eq!(metadata.len(), 1);
    assert_eq!(
        metadata
            .get("acc-exported")
            .and_then(|item| item.note.as_deref()),
        Some("exported note")
    );
    assert!(!metadata.contains_key("acc-ignored"));
}

/// 函数 `single_export_bundle_uses_array_shape_for_reimport`
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
fn single_export_bundle_uses_array_shape_for_reimport() {
    let account = sample_account("acc-1", "first");
    let token = sample_token("acc-1");
    let tokens = HashMap::from([(token.account_id.clone(), token)]);

    let bundle =
        build_single_export_bundle_json(&[account], &tokens, &HashMap::new(), &HashMap::new())
            .expect("build export bundle");
    let content = bundle.content.expect("bundle content");
    let value: serde_json::Value = serde_json::from_slice(&content).expect("parse bundle");

    assert!(value.is_array());
    assert_eq!(value.as_array().map(Vec::len), Some(1));
}

#[test]
fn agent_identity_export_includes_reimportable_credentials() {
    let account = sample_account("acc-agent", "agent");
    let token = sample_token("acc-agent");
    let now = now_ts();
    let identity = AccountAgentIdentity {
        account_id: account.id.clone(),
        agent_runtime_id: "runtime-1".to_string(),
        agent_private_key: "private-key".to_string(),
        task_id: None,
        chatgpt_user_id: "user-1".to_string(),
        chatgpt_account_is_fedramp: true,
        auth_mode: "agentIdentity".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        created_at: now,
        updated_at: now,
    };

    let value = serde_json::to_value(build_account_export_payload(
        &account,
        &token,
        None,
        Some(&identity),
    ))
    .expect("serialize export payload");

    assert_eq!(value["credentials"]["auth_mode"], "agentIdentity");
    assert_eq!(value["credentials"]["agent_runtime_id"], "runtime-1");
    assert_eq!(value["credentials"]["agent_private_key"], "private-key");
    assert!(value["credentials"].get("task_id").is_none());
    assert_eq!(value["credentials"]["chatgpt_user_id"], "user-1");
    assert_eq!(value["credentials"]["chatgpt_account_is_fedramp"], true);
    assert_eq!(value["credentials"]["workspace_id"], "workspace-1");
}
