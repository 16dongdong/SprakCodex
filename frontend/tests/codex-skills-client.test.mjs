import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "codex-skills-client.ts",
);

async function loadClientModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-codex-skills-client-"),
  );
  const tempFile = path.join(tempDir, "codex-skills-client.mjs");
  await fs.writeFile(
    path.join(tempDir, "transport.mjs"),
    "export async function invoke(command, params, options) { globalThis.__codexSkillsInvokeCalls ??= []; globalThis.__codexSkillsInvokeCalls.push({ command, params, options }); return globalThis.__codexSkillsInvokeResult ?? {}; }\nexport function withAddr(value = {}) { return value; }\n",
    "utf8",
  );
  await fs.writeFile(
    tempFile,
    compiled.outputText.replace("./transport", "./transport.mjs"),
    "utf8",
  );
  return import(pathToFileURL(tempFile).href);
}

const client = await loadClientModule();

test("normalizeCodexSkillsInventory accepts camelCase and snake_case payloads", () => {
  const inventory = client.normalizeCodexSkillsInventory({
    codex_home: "/srv/codex",
    skillsRoot: "/srv/codex/skills",
    warnings: [" warning ", null],
    items: [
      {
        directory_name: "user-skill",
        name: " User Skill ",
        description: " description ",
        source: "user",
        deletable: true,
        valid: true,
      },
      {
        directoryName: ".system/system-skill",
        name: "System Skill",
        source: "system",
        deletable: false,
        valid: true,
      },
      { name: "missing-directory" },
    ],
  });

  assert.equal(inventory.codexHome, "/srv/codex");
  assert.equal(inventory.skillsRoot, "/srv/codex/skills");
  assert.deepEqual(inventory.warnings, ["warning"]);
  assert.equal(inventory.items.length, 2);
  assert.deepEqual(inventory.items[0], {
    directoryName: "user-skill",
    name: "User Skill",
    description: "description",
    source: "user",
    deletable: true,
    valid: true,
    error: null,
  });
  assert.equal(inventory.items[1].source, "system");
  assert.equal(inventory.items[1].deletable, false);
});

test("normalizeCodexSkillMarketplaceInventory filters malformed entries", () => {
  const inventory = client.normalizeCodexSkillMarketplaceInventory({
    cli_available: true,
    codex_home: "/srv/codex",
    marketplaces: [
      {
        name: " role-specific-plugins ",
        source_type: "git",
        source: " https://github.com/openai/role-specific-plugins.git ",
      },
      { source: "missing-name" },
    ],
    plugins: [
      {
        plugin_id: "product-design@role-specific-plugins",
        name: " Product Design ",
        marketplace_name: "role-specific-plugins",
        version: "0.1.50",
        installed: true,
        enabled: true,
        description: " Design workflows ",
        author: " OpenAI ",
        category: " Creativity ",
        skills: [
          { name: " audit ", description: " Review a flow " },
          { description: "missing name" },
        ],
      },
      { name: "missing-plugin-id" },
    ],
    warnings: [" warning ", null],
  });

  assert.equal(inventory.cliAvailable, true);
  assert.equal(inventory.codexHome, "/srv/codex");
  assert.deepEqual(inventory.marketplaces, [
    {
      name: "role-specific-plugins",
      sourceType: "git",
      source: "https://github.com/openai/role-specific-plugins.git",
    },
  ]);
  assert.equal(inventory.plugins.length, 1);
  assert.deepEqual(inventory.plugins[0], {
    pluginId: "product-design@role-specific-plugins",
    name: "Product Design",
    marketplaceName: "role-specific-plugins",
    version: "0.1.50",
    installed: true,
    enabled: true,
    description: "Design workflows",
    author: "OpenAI",
    category: "Creativity",
    skills: [{ name: "audit", description: "Review a flow" }],
  });
  assert.deepEqual(inventory.warnings, ["warning"]);
});

test("repository and skills.sh payloads normalize catalog metadata", () => {
  const catalog = client.normalizeCodexSkillRepositoryCatalog({
    repositories: [
      {
        id: "openai-skills",
        name: " OpenAI Skills ",
        owner: "openai",
        repository: "skills",
        source_url: " https://github.com/openai/skills ",
        ref_name: "main",
        is_builtin: true,
        enabled: true,
        skill_count: 1,
        last_scanned_at: "2026-07-23T00:00:00Z",
      },
      { name: "missing id" },
    ],
    items: [
      {
        skill_id: "audit",
        repository_id: "openai-skills",
        name: " Audit ",
        description: " Review a product ",
        repository_name: "OpenAI Skills",
        repository_owner: "openai",
        repository_ref: "main",
        source_url: "https://github.com/openai/skills/tree/main/audit",
        destination_exists: true,
        installs: 128,
        directory_name: "audit",
        path: "audit/SKILL.md",
      },
      { name: "missing skill id" },
    ],
    warnings: [" warning ", null],
  });

  assert.deepEqual(catalog.repositories, [
    {
      id: "openai-skills",
      name: "OpenAI Skills",
      owner: "openai",
      repository: "skills",
      sourceUrl: "https://github.com/openai/skills",
      refName: "main",
      builtin: true,
      enabled: true,
      skillCount: 1,
      lastScannedAt: 1784764800,
      lastError: null,
    },
  ]);
  assert.equal(catalog.items.length, 1);
  assert.equal(catalog.items[0].installed, true);
  assert.equal(catalog.items[0].installs, 128);
  assert.equal(catalog.items[0].installedDirectoryName, "audit");
  assert.deepEqual(catalog.warnings, ["warning"]);

  const registry = client.normalizeCodexSkillRegistrySearchResult({
    items: catalog.items,
    total: 14,
    query: " audit ",
    limit: 12,
    offset: 0,
    warnings: [],
  });
  assert.equal(registry.total, 14);
  assert.equal(registry.query, "audit");
  assert.equal(registry.items[0].skillId, "audit");
});

test("repository and skills.sh methods keep commands and parameters aligned", async () => {
  globalThis.__codexSkillsInvokeCalls = [];
  globalThis.__codexSkillsInvokeResult = {
    repositories: [],
    items: [],
    warnings: [],
    total: 0,
    query: "",
    limit: 48,
    offset: 0,
  };

  await client.codexSkillsClient.listRepositories("/srv/codex");
  await client.codexSkillsClient.addRepository({
    source: "https://github.com/openai/skills",
    refName: "main",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.deleteRepository({
    repositoryId: "openai-skills",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.refreshRepository({
    repositoryId: "openai-skills",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.installRepositorySkill({
    repositoryId: "openai-skills",
    skillId: "audit",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.searchRegistry({
    query: "audit",
    limit: 24,
    offset: 24,
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.installRegistrySkill({
    source: "https://skills.sh/openai/skills/audit",
    skillId: "audit",
    codexHome: "/srv/codex",
  });

  const longOptions = {
    timeoutMs: client.CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS,
    retries: 0,
  };
  assert.equal(client.CODEX_SKILLS_REGISTRY_SEARCH_TIMEOUT_MS, 60_000);
  assert.deepEqual(globalThis.__codexSkillsInvokeCalls, [
    {
      command: "service_codex_skills_repository_list",
      params: { codexHome: "/srv/codex" },
      options: undefined,
    },
    {
      command: "service_codex_skills_repository_add",
      params: {
        source: "https://github.com/openai/skills",
        refName: "main",
        codexHome: "/srv/codex",
      },
      options: longOptions,
    },
    {
      command: "service_codex_skills_repository_delete",
      params: { repositoryId: "openai-skills", codexHome: "/srv/codex" },
      options: longOptions,
    },
    {
      command: "service_codex_skills_repository_refresh",
      params: { repositoryId: "openai-skills", codexHome: "/srv/codex" },
      options: longOptions,
    },
    {
      command: "service_codex_skills_repository_install",
      params: {
        repositoryId: "openai-skills",
        skillId: "audit",
        codexHome: "/srv/codex",
      },
      options: longOptions,
    },
    {
      command: "service_codex_skills_registry_search",
      params: {
        query: "audit",
        limit: 24,
        offset: 24,
        codexHome: "/srv/codex",
      },
      options: {
        timeoutMs: client.CODEX_SKILLS_REGISTRY_SEARCH_TIMEOUT_MS,
        retries: 0,
      },
    },
    {
      command: "service_codex_skills_registry_install",
      params: {
        source: "https://skills.sh/openai/skills/audit",
        skillId: "audit",
        codexHome: "/srv/codex",
      },
      options: longOptions,
    },
  ]);
});

test("marketplace client methods keep command names and RPC parameters aligned", async () => {
  globalThis.__codexSkillsInvokeCalls = [];
  globalThis.__codexSkillsInvokeResult = {
    cliAvailable: true,
    codexHome: "/srv/codex",
    marketplaces: [],
    plugins: [],
    warnings: [],
  };

  await client.codexSkillsClient.listMarketplace("/srv/codex");
  await client.codexSkillsClient.addMarketplace({
    source: "openai/role-specific-plugins",
    refName: "main",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.refreshMarketplace({
    marketplaceName: "role-specific-plugins",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.installMarketplacePlugin({
    pluginId: "product-design@role-specific-plugins",
    codexHome: "/srv/codex",
  });

  const marketplaceOptions = {
    timeoutMs: client.CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS,
    retries: 0,
  };
  assert.equal(client.CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS, 600_000);

  assert.deepEqual(globalThis.__codexSkillsInvokeCalls, [
    {
      command: "service_codex_skills_marketplace_list",
      params: { codexHome: "/srv/codex" },
      options: marketplaceOptions,
    },
    {
      command: "service_codex_skills_marketplace_add",
      params: {
        source: "openai/role-specific-plugins",
        refName: "main",
        codexHome: "/srv/codex",
      },
      options: marketplaceOptions,
    },
    {
      command: "service_codex_skills_marketplace_refresh",
      params: {
        marketplaceName: "role-specific-plugins",
        codexHome: "/srv/codex",
      },
      options: marketplaceOptions,
    },
    {
      command: "service_codex_skills_marketplace_plugin_install",
      params: {
        pluginId: "product-design@role-specific-plugins",
        codexHome: "/srv/codex",
      },
      options: marketplaceOptions,
    },
  ]);
});

test("Skill file mutations use a long non-retrying request", async () => {
  globalThis.__codexSkillsInvokeCalls = [];
  globalThis.__codexSkillsInvokeResult = {
    codexHome: "/srv/codex",
    skillsRoot: "/srv/codex/skills",
    items: [],
    warnings: [],
  };

  await client.codexSkillsClient.list("/srv/codex");
  await client.codexSkillsClient.installZip({
    fileName: "skill.zip",
    archiveBase64: "eA==",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.importDirectory({
    sourcePath: "/opt/skills/example",
    codexHome: "/srv/codex",
  });
  await client.codexSkillsClient.delete({
    directoryName: "example",
    codexHome: "/srv/codex",
  });

  const longOptions = {
    timeoutMs: client.CODEX_SKILLS_LONG_OPERATION_TIMEOUT_MS,
    retries: 0,
  };
  assert.deepEqual(
    globalThis.__codexSkillsInvokeCalls.map(({ command, options }) => ({
      command,
      options,
    })),
    [
      { command: "service_codex_skills_list", options: undefined },
      { command: "service_codex_skills_install_zip", options: longOptions },
      {
        command: "service_codex_skills_import_directory",
        options: longOptions,
      },
      { command: "service_codex_skills_delete", options: longOptions },
    ],
  );
});

test("skills page separates Skills installation from Codex plugin installation", async () => {
  const pageSource = await fs.readFile(
    path.join(appsRoot, "src", "app", "skills", "page.tsx"),
    "utf8",
  );
  const panelSource = await fs.readFile(
    path.join(appsRoot, "src", "app", "skills", "marketplace-dialog.tsx"),
    "utf8",
  );
  const catalogSource = await fs.readFile(
    path.join(appsRoot, "src", "app", "skills", "skills-catalog-panel.tsx"),
    "utf8",
  );
  const repositoriesSource = await fs.readFile(
    path.join(
      appsRoot,
      "src",
      "app",
      "skills",
      "skill-repositories-dialog.tsx",
    ),
    "utf8",
  );

  assert.match(pageSource, /<Tabs[\s\S]*?value=\{activeTab\}/);
  assert.match(pageSource, /<TabsTrigger value="skills"/);
  assert.match(pageSource, /<TabsTrigger value="plugins"/);
  assert.match(pageSource, /<TabsContent value="skills"/);
  assert.match(pageSource, /<TabsContent value="plugins" keepMounted>/);
  assert.match(pageSource, /<CodexPluginsPanel active=\{activeTab === "plugins"\}/);
  assert.match(pageSource, /t\("Skills 安装"\)/);
  assert.match(pageSource, /t\("Codex 插件安装"\)/);
  assert.match(pageSource, /<SkillsCatalogPanel/);
  assert.match(catalogSource, /<TabsTrigger value="repositories"/);
  assert.match(catalogSource, /<TabsTrigger value="registry"/);
  assert.match(catalogSource, /skills\.sh/);
  assert.match(catalogSource, /installRepositorySkill/);
  assert.match(catalogSource, /installRegistrySkill/);
  assert.match(
    catalogSource,
    /repository\.skillCount === 0 && repository\.lastScannedAt === null/,
  );
  assert.match(catalogSource, /initialRefreshAttemptedRef\.current = true/);
  assert.match(catalogSource, /codexSkillsClient\.refreshRepository\(\)/);
  assert.match(catalogSource, /首次进入，正在后台同步技能仓库/);
  assert.match(catalogSource, /onImportDirectory/);
  assert.match(catalogSource, /onInstallZip/);
  assert.match(repositoriesSource, /codexSkillsClient\.addRepository/);
  assert.match(repositoriesSource, /codexSkillsClient\.refreshRepository/);
  assert.match(repositoriesSource, /codexSkillsClient\.deleteRepository/);
  assert.match(repositoriesSource, /删除仓库不会删除已安装的 Skills/);
  assert.doesNotMatch(pageSource, /SkillsMarketplaceDialog|marketplaceDialogOpen/);
  assert.match(panelSource, /export function CodexPluginsPanel/);
  assert.match(panelSource, /enabled: active && enabled/);
  assert.match(panelSource, /插件中的 Skills 会随完整插件一起安装，不能在这里单独安装。/);
  assert.match(panelSource, /marketplaceSource=\{/);
  assert.match(panelSource, /来源：\{source\}/);
  assert.match(panelSource, /<ConfirmDialog/);
});

test("inline plugin marketplace keeps its list scrollable and installation errors compact", async () => {
  const panelSource = await fs.readFile(
    path.join(appsRoot, "src", "app", "skills", "marketplace-dialog.tsx"),
    "utf8",
  );
  const globalStyles = await fs.readFile(
    path.join(appsRoot, "src", "app", "globals.css"),
    "utf8",
  );
  const scrollAreaSource = await fs.readFile(
    path.join(appsRoot, "src", "components", "ui", "scroll-area.tsx"),
    "utf8",
  );

  assert.match(panelSource, /data-testid="codex-plugins-panel"/);
  assert.match(panelSource, /data-testid="skills-marketplace-scroll"/);
  assert.match(panelSource, /<ScrollArea/);
  assert.match(panelSource, /height: "min\(64vh, 680px\)"/);
  assert.match(panelSource, /keepScrollbarMounted/);
  assert.match(panelSource, /scrollbarClassName="skills-marketplace-scrollbar"/);
  assert.match(
    panelSource,
    /Number\(right\.installed\) - Number\(left\.installed\)/,
  );
  assert.match(panelSource, /已安装 \{count\}/);
  assert.match(scrollAreaSource, /keepScrollbarMounted = false/);
  assert.match(scrollAreaSource, /keepMounted=\{keepScrollbarMounted\}/);
  assert.match(
    globalStyles,
    /\.skills-marketplace-scrollbar\s*\{[\s\S]*?width: 12px !important;[\s\S]*?visibility: visible !important;/,
  );
  assert.match(
    globalStyles,
    /\.skills-marketplace-scrollbar-thumb\s*\{[\s\S]*?min-height: 52px;/,
  );
  assert.match(
    panelSource,
    /toast\.error\(t\("安装插件失败"\),\s*\{[\s\S]*?description: getAppErrorMessage\(error\),[\s\S]*?className: "skills-marketplace-install-error-toast"/,
  );
  assert.match(
    globalStyles,
    /\.skills-marketplace-install-error-toast\s*\{[\s\S]*?width: min\(26rem, calc\(100vw - 2rem\)\)[\s\S]*?max-height: min\(26rem, calc\(100dvh - 2rem\)\)/,
  );
  assert.match(
    globalStyles,
    /\.skills-marketplace-install-error-toast \[data-description\]\s*\{[\s\S]*?overflow-y: auto/,
  );
});
