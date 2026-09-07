export type CodexSkillSource = "user" | "system";

export interface CodexSkillSummary {
  directoryName: string;
  name: string;
  description: string;
  source: CodexSkillSource;
  deletable: boolean;
  valid: boolean;
  error: string | null;
}

export interface CodexSkillsInventory {
  codexHome: string;
  skillsRoot: string;
  items: CodexSkillSummary[];
  warnings: string[];
}

export interface CodexSkillRepositorySummary {
  id: string;
  name: string;
  owner: string;
  repository: string;
  sourceUrl: string;
  refName: string;
  builtin: boolean;
  enabled: boolean;
  skillCount: number;
  lastScannedAt: number | null;
  lastError: string | null;
}

export interface CodexSkillCatalogItem {
  skillId: string;
  repositoryId: string;
  name: string;
  description: string;
  author: string;
  category: string;
  path: string;
  repositoryName: string;
  repositoryOwner: string;
  repositoryRef: string;
  sourceUrl: string;
  installs: number;
  installed: boolean;
  installedDirectoryName: string | null;
}

export interface CodexSkillRepositoryCatalog {
  repositories: CodexSkillRepositorySummary[];
  items: CodexSkillCatalogItem[];
  warnings: string[];
}

export interface CodexSkillRegistrySearchResult {
  items: CodexSkillCatalogItem[];
  total: number;
  query: string;
  limit: number;
  offset: number;
  warnings: string[];
}

export interface CodexSkillMarketplaceSummary {
  name: string;
  sourceType: string;
  source: string | null;
}

export interface CodexSkillMarketplaceSkill {
  name: string;
  description: string;
}

export interface CodexSkillMarketplacePlugin {
  pluginId: string;
  name: string;
  marketplaceName: string;
  version: string;
  installed: boolean;
  enabled: boolean;
  description: string;
  author: string;
  category: string;
  skills: CodexSkillMarketplaceSkill[];
}

export interface CodexSkillMarketplaceInventory {
  cliAvailable: boolean;
  codexHome: string;
  marketplaces: CodexSkillMarketplaceSummary[];
  plugins: CodexSkillMarketplacePlugin[];
  warnings: string[];
}
