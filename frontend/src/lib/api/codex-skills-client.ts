import type {
  CodexSkillCatalogItem,
  CodexSkillMarketplaceInventory,
  CodexSkillMarketplacePlugin,
  CodexSkillMarketplaceSkill,
  CodexSkillMarketplaceSummary,
  CodexSkillRegistrySearchResult,
  CodexSkillRepositoryCatalog,
  CodexSkillRepositorySummary,
  CodexSkillSource,
  CodexSkillSummary,
  CodexSkillsInventory,
} from "@/types";
import { invoke, withAddr } from "./transport";

export const CODEX_SKILLS_QUERY_KEY = ["codex-skills", "inventory"] as const;
export const CODEX_SKILLS_MARKETPLACE_QUERY_KEY = [
  "codex-skills",
  "marketplace",
] as const;
export const CODEX_SKILLS_REPOSITORIES_QUERY_KEY = [
  "codex-skills",
  "repositories",
] as const;
export const CODEX_SKILLS_REGISTRY_QUERY_KEY = [
  "codex-skills",
  "registry",
] as const;
export const MAX_CODEX_SKILL_ZIP_BYTES = 16 * 1024 * 1024;
export const CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS = 10 * 60 * 1000;
export const CODEX_SKILLS_REGISTRY_SEARCH_TIMEOUT_MS = 60 * 1000;

function asObject(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function asString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function asBoolean(value: unknown): boolean {
  return value === true;
}

function asNumber(value: unknown): number {
  const number = typeof value === "number" ? value : Number(value);
  return Number.isFinite(number) ? number : 0;
}

function asNullableString(value: unknown): string | null {
  return asString(value) || null;
}

function asNullableEpochSeconds(value: unknown): number | null {
  const numeric = asNumber(value);
  if (numeric > 0) return Math.trunc(numeric);
  const text = asString(value);
  if (!text) return null;
  const parsed = Date.parse(text);
  return Number.isFinite(parsed) && parsed > 0
    ? Math.trunc(parsed / 1_000)
    : null;
}

function normalizeSource(value: unknown): CodexSkillSource {
  return asString(value).toLowerCase() === "system" ? "system" : "user";
}

function normalizeSkill(payload: unknown): CodexSkillSummary | null {
  const source = asObject(payload);
  const directoryName = asString(source.directoryName ?? source.directory_name);
  if (!directoryName) return null;
  return {
    directoryName,
    name: asString(source.name) || directoryName,
    description: asString(source.description),
    source: normalizeSource(source.source),
    deletable: asBoolean(source.deletable),
    valid: asBoolean(source.valid),
    error: asString(source.error) || null,
  };
}

export function normalizeCodexSkillsInventory(
  payload: unknown,
): CodexSkillsInventory {
  const source = asObject(payload);
  return {
    codexHome: asString(source.codexHome ?? source.codex_home),
    skillsRoot: asString(source.skillsRoot ?? source.skills_root),
    items: (Array.isArray(source.items) ? source.items : [])
      .map(normalizeSkill)
      .filter((item): item is CodexSkillSummary => Boolean(item)),
    warnings: (Array.isArray(source.warnings) ? source.warnings : [])
      .map(asString)
      .filter(Boolean),
  };
}

function normalizeRepository(
  payload: unknown,
): CodexSkillRepositorySummary | null {
  const source = asObject(payload);
  const id = asString(source.id);
  if (!id) return null;
  const repository = asString(source.repository ?? source.repo);
  const owner = asString(source.owner);
  return {
    id,
    name: asString(source.name) || [owner, repository].filter(Boolean).join("/"),
    owner,
    repository,
    sourceUrl: asString(source.sourceUrl ?? source.source_url),
    refName: asString(source.refName ?? source.ref_name),
    builtin: asBoolean(source.builtin ?? source.isBuiltin ?? source.is_builtin),
    enabled: source.enabled !== false,
    skillCount: Math.max(
      0,
      Math.trunc(asNumber(source.skillCount ?? source.skill_count)),
    ),
    lastScannedAt: asNullableEpochSeconds(
      source.lastScannedAt ?? source.last_scanned_at,
    ),
    lastError: asNullableString(source.lastError ?? source.last_error),
  };
}

function normalizeCatalogItem(payload: unknown): CodexSkillCatalogItem | null {
  const source = asObject(payload);
  const skillId = asString(source.skillId ?? source.skill_id);
  if (!skillId) return null;
  const repositoryId = asString(source.repositoryId ?? source.repository_id);
  return {
    skillId,
    repositoryId,
    name: asString(source.name) || skillId,
    description: asString(source.description),
    author: asString(source.author),
    category: asString(source.category),
    path: asString(source.path),
    repositoryName:
      asString(source.repositoryName ?? source.repository_name) || repositoryId,
    repositoryOwner: asString(
      source.repositoryOwner ?? source.repository_owner,
    ),
    repositoryRef: asString(source.repositoryRef ?? source.repository_ref),
    sourceUrl: asString(source.sourceUrl ?? source.source_url ?? source.source),
    installs: Math.max(0, Math.trunc(asNumber(source.installs))),
    installed: asBoolean(
      source.installed ?? source.destinationExists ?? source.destination_exists,
    ),
    installedDirectoryName: asNullableString(
      source.installedDirectoryName ??
        source.installed_directory_name ??
        source.directoryName ??
        source.directory_name,
    ),
  };
}

export function normalizeCodexSkillRepositoryCatalog(
  payload: unknown,
): CodexSkillRepositoryCatalog {
  const source = asObject(payload);
  return {
    repositories: (Array.isArray(source.repositories) ? source.repositories : [])
      .map(normalizeRepository)
      .filter((item): item is CodexSkillRepositorySummary => Boolean(item)),
    items: (Array.isArray(source.items) ? source.items : [])
      .map(normalizeCatalogItem)
      .filter((item): item is CodexSkillCatalogItem => Boolean(item)),
    warnings: (Array.isArray(source.warnings) ? source.warnings : [])
      .map(asString)
      .filter(Boolean),
  };
}

export function normalizeCodexSkillRegistrySearchResult(
  payload: unknown,
): CodexSkillRegistrySearchResult {
  const source = asObject(payload);
  const items = (Array.isArray(source.items) ? source.items : [])
    .map(normalizeCatalogItem)
    .filter((item): item is CodexSkillCatalogItem => Boolean(item));
  return {
    items,
    total: Math.max(0, Math.trunc(asNumber(source.total) || items.length)),
    query: asString(source.query),
    limit: Math.max(0, Math.trunc(asNumber(source.limit))),
    offset: Math.max(0, Math.trunc(asNumber(source.offset))),
    warnings: (Array.isArray(source.warnings) ? source.warnings : [])
      .map(asString)
      .filter(Boolean),
  };
}

function normalizeMarketplaceSummary(
  payload: unknown,
): CodexSkillMarketplaceSummary | null {
  const source = asObject(payload);
  const name = asString(source.name);
  if (!name) return null;
  return {
    name,
    sourceType: asString(source.sourceType ?? source.source_type),
    source: asNullableString(source.source),
  };
}

function normalizeMarketplaceSkill(
  payload: unknown,
): CodexSkillMarketplaceSkill | null {
  const source = asObject(payload);
  const name = asString(source.name);
  if (!name) return null;
  return {
    name,
    description: asString(source.description),
  };
}

function normalizeMarketplacePlugin(
  payload: unknown,
): CodexSkillMarketplacePlugin | null {
  const source = asObject(payload);
  const pluginId = asString(source.pluginId ?? source.plugin_id);
  if (!pluginId) return null;
  return {
    pluginId,
    name: asString(source.name) || pluginId,
    marketplaceName: asString(
      source.marketplaceName ?? source.marketplace_name,
    ),
    version: asString(source.version),
    installed: asBoolean(source.installed),
    enabled: asBoolean(source.enabled),
    description: asString(source.description),
    author: asString(source.author),
    category: asString(source.category),
    skills: (Array.isArray(source.skills) ? source.skills : [])
      .map(normalizeMarketplaceSkill)
      .filter((item): item is CodexSkillMarketplaceSkill => Boolean(item)),
  };
}

export function normalizeCodexSkillMarketplaceInventory(
  payload: unknown,
): CodexSkillMarketplaceInventory {
  const source = asObject(payload);
  return {
    cliAvailable: asBoolean(source.cliAvailable ?? source.cli_available),
    codexHome: asString(source.codexHome ?? source.codex_home),
    marketplaces: (Array.isArray(source.marketplaces)
      ? source.marketplaces
      : []
    )
      .map(normalizeMarketplaceSummary)
      .filter((item): item is CodexSkillMarketplaceSummary => Boolean(item)),
    plugins: (Array.isArray(source.plugins) ? source.plugins : [])
      .map(normalizeMarketplacePlugin)
      .filter((item): item is CodexSkillMarketplacePlugin => Boolean(item)),
    warnings: (Array.isArray(source.warnings) ? source.warnings : [])
      .map(asString)
      .filter(Boolean),
  };
}

async function invokeInventory(
  command: string,
  params: Record<string, unknown> = {},
): Promise<CodexSkillsInventory> {
  const isMutation = command !== "service_codex_skills_list";
  const result = await invoke<unknown>(
    command,
    withAddr(params),
    isMutation
      ? {
          timeoutMs: CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS,
          // File mutations may complete on the service after the browser stops waiting. Retrying
          // would repeat an install/import/delete and turn a successful first attempt into an error.
          retries: 0,
        }
      : undefined,
  );
  return normalizeCodexSkillsInventory(result);
}

async function invokeMarketplace(
  command: string,
  params: Record<string, unknown> = {},
): Promise<CodexSkillMarketplaceInventory> {
  const result = await invoke<unknown>(command, withAddr(params), {
    timeoutMs: CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS,
    // A timed-out mutation may still be running on the service host. Retrying it would enqueue a
    // duplicate Marketplace operation, so React Query owns any user-visible retry instead.
    retries: 0,
  });
  return normalizeCodexSkillMarketplaceInventory(result);
}

async function invokeRepositoryCatalog(
  command: string,
  params: Record<string, unknown> = {},
): Promise<CodexSkillRepositoryCatalog> {
  const result = await invoke<unknown>(command, withAddr(params),
    command === "service_codex_skills_repository_list"
      ? undefined
      : { timeoutMs: CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS, retries: 0 },
  );
  return normalizeCodexSkillRepositoryCatalog(result);
}

export const codexSkillsClient = {
  list(codexHome?: string | null): Promise<CodexSkillsInventory> {
    return invokeInventory("service_codex_skills_list", {
      codexHome: codexHome || null,
    });
  },

  installZip(params: {
    fileName: string;
    archiveBase64: string;
    codexHome?: string | null;
  }): Promise<CodexSkillsInventory> {
    return invokeInventory("service_codex_skills_install_zip", {
      fileName: params.fileName,
      archiveBase64: params.archiveBase64,
      codexHome: params.codexHome || null,
    });
  },

  importDirectory(params: {
    sourcePath: string;
    codexHome?: string | null;
  }): Promise<CodexSkillsInventory> {
    return invokeInventory("service_codex_skills_import_directory", {
      sourcePath: params.sourcePath,
      codexHome: params.codexHome || null,
    });
  },

  delete(params: {
    directoryName: string;
    codexHome?: string | null;
  }): Promise<CodexSkillsInventory> {
    return invokeInventory("service_codex_skills_delete", {
      directoryName: params.directoryName,
      codexHome: params.codexHome || null,
    });
  },

  listRepositories(
    codexHome?: string | null,
  ): Promise<CodexSkillRepositoryCatalog> {
    return invokeRepositoryCatalog("service_codex_skills_repository_list", {
      codexHome: codexHome || null,
    });
  },

  addRepository(params: {
    source: string;
    refName?: string | null;
    codexHome?: string | null;
  }): Promise<CodexSkillRepositoryCatalog> {
    return invokeRepositoryCatalog("service_codex_skills_repository_add", {
      source: params.source,
      refName: params.refName || null,
      codexHome: params.codexHome || null,
    });
  },

  deleteRepository(params: {
    repositoryId: string;
    codexHome?: string | null;
  }): Promise<CodexSkillRepositoryCatalog> {
    return invokeRepositoryCatalog("service_codex_skills_repository_delete", {
      repositoryId: params.repositoryId,
      codexHome: params.codexHome || null,
    });
  },

  refreshRepository(params?: {
    repositoryId?: string | null;
    codexHome?: string | null;
  }): Promise<CodexSkillRepositoryCatalog> {
    return invokeRepositoryCatalog("service_codex_skills_repository_refresh", {
      repositoryId: params?.repositoryId || null,
      codexHome: params?.codexHome || null,
    });
  },

  installRepositorySkill(params: {
    repositoryId: string;
    skillId: string;
    codexHome?: string | null;
  }): Promise<CodexSkillRepositoryCatalog> {
    return invokeRepositoryCatalog("service_codex_skills_repository_install", {
      repositoryId: params.repositoryId,
      skillId: params.skillId,
      codexHome: params.codexHome || null,
    });
  },

  async searchRegistry(params?: {
    query?: string;
    limit?: number;
    offset?: number;
    codexHome?: string | null;
  }): Promise<CodexSkillRegistrySearchResult> {
    const result = await invoke<unknown>(
      "service_codex_skills_registry_search",
      withAddr({
        query: params?.query?.trim() || "",
        limit: params?.limit ?? 48,
        offset: params?.offset ?? 0,
        codexHome: params?.codexHome || null,
      }),
      {
        timeoutMs: CODEX_SKILLS_REGISTRY_SEARCH_TIMEOUT_MS,
        retries: 0,
      },
    );
    return normalizeCodexSkillRegistrySearchResult(result);
  },

  installRegistrySkill(params: {
    source: string;
    skillId: string;
    codexHome?: string | null;
  }): Promise<CodexSkillsInventory> {
    return invokeInventory("service_codex_skills_registry_install", {
      source: params.source,
      skillId: params.skillId,
      codexHome: params.codexHome || null,
    });
  },

  listMarketplace(
    codexHome?: string | null,
  ): Promise<CodexSkillMarketplaceInventory> {
    return invokeMarketplace("service_codex_skills_marketplace_list", {
      codexHome: codexHome || null,
    });
  },

  addMarketplace(params: {
    source: string;
    refName?: string | null;
    codexHome?: string | null;
  }): Promise<CodexSkillMarketplaceInventory> {
    return invokeMarketplace("service_codex_skills_marketplace_add", {
      source: params.source,
      refName: params.refName || null,
      codexHome: params.codexHome || null,
    });
  },

  refreshMarketplace(params?: {
    marketplaceName?: string | null;
    codexHome?: string | null;
  }): Promise<CodexSkillMarketplaceInventory> {
    return invokeMarketplace("service_codex_skills_marketplace_refresh", {
      marketplaceName: params?.marketplaceName || null,
      codexHome: params?.codexHome || null,
    });
  },

  installMarketplacePlugin(params: {
    pluginId: string;
    codexHome?: string | null;
  }): Promise<CodexSkillMarketplaceInventory> {
    return invokeMarketplace(
      "service_codex_skills_marketplace_plugin_install",
      {
        pluginId: params.pluginId,
        codexHome: params.codexHome || null,
      },
    );
  },
};
