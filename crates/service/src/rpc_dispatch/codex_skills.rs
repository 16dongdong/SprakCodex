use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "codexSkills/list" => super::value_or_error(crate::codex_skills::list(super::str_param(
            req,
            "codexHome",
        ))),
        "codexSkills/installZip" => super::value_or_error(crate::codex_skills::install_zip(
            super::str_param(req, "fileName"),
            super::str_param(req, "archiveBase64"),
            super::str_param(req, "codexHome"),
        )),
        "codexSkills/importDirectory" => {
            super::value_or_error(crate::codex_skills::import_directory(
                super::str_param(req, "sourcePath"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/delete" => super::value_or_error(crate::codex_skills::delete(
            super::str_param(req, "directoryName"),
            super::str_param(req, "codexHome"),
        )),
        "codexSkills/repositoryList" => super::value_or_error(
            crate::codex_skill_repositories::list(super::str_param(req, "codexHome")),
        ),
        "codexSkills/repositoryAdd" => super::value_or_error(crate::codex_skill_repositories::add(
            super::str_param(req, "source"),
            super::str_param(req, "refName"),
            super::str_param(req, "codexHome"),
        )),
        "codexSkills/repositoryDelete" => {
            super::value_or_error(crate::codex_skill_repositories::delete(
                super::str_param(req, "repositoryId"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/repositoryRefresh" => {
            super::value_or_error(crate::codex_skill_repositories::refresh(
                super::str_param(req, "repositoryId"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/repositoryInstall" => {
            super::value_or_error(crate::codex_skill_repositories::install(
                super::str_param(req, "repositoryId"),
                super::str_param(req, "skillId"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/registrySearch" => {
            super::value_or_error(crate::codex_skill_repositories::registry_search(
                super::str_param(req, "query"),
                super::i64_param(req, "limit"),
                super::i64_param(req, "offset"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/registryInstall" => {
            super::value_or_error(crate::codex_skill_repositories::registry_install(
                super::str_param(req, "source"),
                super::str_param(req, "skillId"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/marketplaceList" => super::value_or_error(
            crate::codex_skills_marketplace::list(super::str_param(req, "codexHome")),
        ),
        "codexSkills/marketplaceAdd" => {
            super::value_or_error(crate::codex_skills_marketplace::add(
                super::str_param(req, "source"),
                super::str_param(req, "refName"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/marketplaceRefresh" => {
            super::value_or_error(crate::codex_skills_marketplace::refresh(
                super::str_param(req, "marketplaceName"),
                super::str_param(req, "codexHome"),
            ))
        }
        "codexSkills/marketplacePluginInstall" => {
            super::value_or_error(crate::codex_skills_marketplace::install(
                super::str_param(req, "pluginId"),
                super::str_param(req, "codexHome"),
            ))
        }
        _ => return None,
    };

    Some(super::response(req, result))
}
