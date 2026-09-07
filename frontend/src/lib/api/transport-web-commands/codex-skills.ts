import type { WebCommandDescriptor } from "./shared";

export function createCodexSkillsWebCommands(): Record<
  string,
  WebCommandDescriptor
> {
  return {
    service_codex_skills_list: { rpcMethod: "codexSkills/list" },
    service_codex_skills_install_zip: { rpcMethod: "codexSkills/installZip" },
    service_codex_skills_import_directory: {
      rpcMethod: "codexSkills/importDirectory",
    },
    service_codex_skills_delete: { rpcMethod: "codexSkills/delete" },
    service_codex_skills_repository_list: {
      rpcMethod: "codexSkills/repositoryList",
    },
    service_codex_skills_repository_add: {
      rpcMethod: "codexSkills/repositoryAdd",
    },
    service_codex_skills_repository_delete: {
      rpcMethod: "codexSkills/repositoryDelete",
    },
    service_codex_skills_repository_refresh: {
      rpcMethod: "codexSkills/repositoryRefresh",
    },
    service_codex_skills_repository_install: {
      rpcMethod: "codexSkills/repositoryInstall",
    },
    service_codex_skills_registry_search: {
      rpcMethod: "codexSkills/registrySearch",
    },
    service_codex_skills_registry_install: {
      rpcMethod: "codexSkills/registryInstall",
    },
    service_codex_skills_marketplace_list: {
      rpcMethod: "codexSkills/marketplaceList",
    },
    service_codex_skills_marketplace_add: {
      rpcMethod: "codexSkills/marketplaceAdd",
    },
    service_codex_skills_marketplace_refresh: {
      rpcMethod: "codexSkills/marketplaceRefresh",
    },
    service_codex_skills_marketplace_plugin_install: {
      rpcMethod: "codexSkills/marketplacePluginInstall",
    },
  };
}
