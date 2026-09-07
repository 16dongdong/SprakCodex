use crate::commands::shared::rpc_call_in_background;

#[tauri::command]
pub async fn service_codex_skills_list(
    addr: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "codexHome": codex_home });
    rpc_call_in_background("codexSkills/list", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_install_zip(
    addr: Option<String>,
    file_name: String,
    archive_base64: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "fileName": file_name,
        "archiveBase64": archive_base64,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/installZip", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_import_directory(
    addr: Option<String>,
    source_path: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "sourcePath": source_path,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/importDirectory", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_delete(
    addr: Option<String>,
    directory_name: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "directoryName": directory_name,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/delete", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_repository_list(
    addr: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "codexHome": codex_home });
    rpc_call_in_background("codexSkills/repositoryList", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_repository_add(
    addr: Option<String>,
    source: String,
    ref_name: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "source": source,
        "refName": ref_name,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/repositoryAdd", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_repository_delete(
    addr: Option<String>,
    repository_id: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "repositoryId": repository_id,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/repositoryDelete", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_repository_refresh(
    addr: Option<String>,
    repository_id: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "repositoryId": repository_id,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/repositoryRefresh", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_repository_install(
    addr: Option<String>,
    repository_id: String,
    skill_id: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "repositoryId": repository_id,
        "skillId": skill_id,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/repositoryInstall", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_registry_search(
    addr: Option<String>,
    query: String,
    limit: Option<i64>,
    offset: Option<i64>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "query": query,
        "limit": limit,
        "offset": offset,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/registrySearch", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_registry_install(
    addr: Option<String>,
    source: String,
    skill_id: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "source": source,
        "skillId": skill_id,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/registryInstall", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_marketplace_list(
    addr: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "codexHome": codex_home });
    rpc_call_in_background("codexSkills/marketplaceList", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_marketplace_add(
    addr: Option<String>,
    source: String,
    ref_name: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "source": source,
        "refName": ref_name,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/marketplaceAdd", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_marketplace_refresh(
    addr: Option<String>,
    marketplace_name: Option<String>,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "marketplaceName": marketplace_name,
        "codexHome": codex_home,
    });
    rpc_call_in_background("codexSkills/marketplaceRefresh", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_codex_skills_marketplace_plugin_install(
    addr: Option<String>,
    plugin_id: String,
    codex_home: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "pluginId": plugin_id,
        "codexHome": codex_home,
    });
    rpc_call_in_background(
        "codexSkills/marketplacePluginInstall",
        addr,
        Some(params),
    )
    .await
}
