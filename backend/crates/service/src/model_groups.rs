use std::collections::HashSet;

use codexmanager_core::storage::{now_ts, ApiKeyOwner, ModelGroupAccess, Storage};

fn user_owner(owner: &ApiKeyOwner) -> Option<&str> {
    if owner.owner_kind != "user" {
        return None;
    }
    owner
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn resolve_api_key_model_group_access(
    storage: &Storage,
    key_id: &str,
    platform_model_slug: &str,
) -> Result<Option<ModelGroupAccess>, String> {
    let requested_model_slug = platform_model_slug.trim();
    let catalog_model_slug = crate::models_v2::policy_catalog_slug(requested_model_slug);
    let owner = storage
        .find_api_key_owner(key_id)
        .map_err(|err| format!("read api key owner failed: {err}"))?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    let Some(user_id) = user_owner(&owner) else {
        return Ok(None);
    };
    let user = storage
        .find_app_user_access_summary_by_id(user_id)
        .map_err(|err| format!("read app user failed: {err}"))?
        .ok_or_else(|| "API Key 归属用户不存在".to_string())?;
    if user.role == "admin" {
        return Ok(None);
    }
    if user.status != "active" {
        return Err("API Key 归属用户已停用".to_string());
    }
    let access = storage
        .resolve_model_group_access_for_user_v2(user_id, catalog_model_slug, now_ts())
        .map_err(|err| format!("read model group access failed: {err}"))?;
    match access {
        Some(access) => Ok(Some(access)),
        None => Err(format!("model_not_allowed: {requested_model_slug}")),
    }
}

pub(crate) fn allowed_model_slugs_for_api_key(
    storage: &Storage,
    key_id: &str,
) -> Result<Option<HashSet<String>>, String> {
    let owner = storage
        .find_api_key_owner(key_id)
        .map_err(|err| format!("read api key owner failed: {err}"))?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    let Some(user_id) = user_owner(&owner) else {
        return Ok(None);
    };
    let user = storage
        .find_app_user_access_summary_by_id(user_id)
        .map_err(|err| format!("read app user failed: {err}"))?
        .ok_or_else(|| "API Key 归属用户不存在".to_string())?;
    if user.role == "admin" {
        return Ok(None);
    }
    let slugs = storage
        .allowed_model_slugs_for_user_v2(user_id, now_ts())
        .map_err(|err| format!("read allowed model groups failed: {err}"))?
        .into_iter()
        .collect::<HashSet<_>>();
    Ok(Some(slugs))
}
