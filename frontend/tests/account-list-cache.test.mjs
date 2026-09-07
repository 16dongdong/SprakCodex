import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { QueryClient } from "@tanstack/react-query";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");

async function readSource(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

async function loadQueryKeysModule() {
  const sourcePath = path.join(
    appsRoot,
    "src",
    "lib",
    "api",
    "account-query-keys.ts",
  );
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-account-query-keys-"),
  );
  const tempFile = path.join(tempDir, "account-query-keys.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const queryKeys = await loadQueryKeysModule();

function readConstFunctionBody(source, functionName) {
  const start = source.indexOf(`const ${functionName} = async () => {`);
  assert.notEqual(start, -1, `${functionName} not found`);
  const end = source.slice(start).search(/\r?\n[\t ]*\};\r?\n/);
  assert.notEqual(end, -1, `${functionName} body end not found`);
  return source.slice(start, start + end);
}

test("账号实体列表不会被用量刷新路径自动打空", async () => {
  const source = await readSource("src/hooks/useAccounts.ts");
  const invalidateUsageBody = readConstFunctionBody(source, "invalidateUsageData");

  assert.match(
    source,
    /queryKey:\s*accountListQueryKey[\s\S]*staleTime:\s*Infinity/,
  );
  assert.doesNotMatch(invalidateUsageBody, /queryKey:\s*\[\s*"accounts"/);
  assert.match(
    source,
    /const refreshAccountMutation = useMutation\(\{[\s\S]*onSettled:\s*async \(\) => \{[\s\S]*await invalidateUsageData\(\);/,
  );
  assert.match(
    source,
    /const refreshAllMutation = useMutation\(\{[\s\S]*onSettled:\s*async \(\) => \{[\s\S]*await invalidateUsageData\(\);/,
  );
});

test("账号页用启动快照作为账号实体列表的非空初始来源", async () => {
  const source = await readSource("src/hooks/useAccounts.ts");

  assert.match(source, /const startupSnapshotQuery = useQuery\(\{/);
  assert.match(source, /buildAccountListResultFromSnapshot\(startupAccounts\)/);
  assert.match(
    source,
    /account\/list returned empty while startup snapshot still has accounts/,
  );
  assert.match(source, /initialData:\s*\(\) =>[\s\S]*startupAccountList/);
});

test("账号和用量查询键按服务地址隔离", () => {
  const accountA = queryKeys.buildAccountListQueryKey("127.0.0.1:48760");
  const accountB = queryKeys.buildAccountListQueryKey("127.0.0.1:58760");
  const usageA = queryKeys.buildAccountUsageListQueryKey("127.0.0.1:48760");
  const usageB = queryKeys.buildAccountUsageListQueryKey("127.0.0.1:58760");

  assert.deepEqual(accountA, ["accounts", "list", "127.0.0.1:48760"]);
  assert.deepEqual(accountB, ["accounts", "list", "127.0.0.1:58760"]);
  assert.deepEqual(usageA, ["usage", "list", "127.0.0.1:48760"]);
  assert.deepEqual(usageB, ["usage", "list", "127.0.0.1:58760"]);
  assert.notDeepEqual(accountA, accountB);
  assert.notDeepEqual(usageA, usageB);
  assert.deepEqual(queryKeys.buildAccountListQueryKey("  service-a  "), [
    "accounts",
    "list",
    "service-a",
  ]);
  assert.deepEqual(queryKeys.buildAccountListQueryKey("   "), [
    "accounts",
    "list",
    null,
  ]);
});

test("平台密钥和模型选择器查询键按服务地址隔离", () => {
  const apiKeysA = queryKeys.buildApiKeyListQueryKey("service-a");
  const apiKeysB = queryKeys.buildApiKeyListQueryKey("service-b");
  const lookupA = queryKeys.buildApiKeyLookupQueryKey("service-a");
  const lookupB = queryKeys.buildApiKeyLookupQueryKey("service-b");
  const modelsA = queryKeys.buildManagedModelSelectorQueryKey("service-a");
  const modelsB = queryKeys.buildManagedModelSelectorQueryKey("service-b");

  assert.deepEqual(apiKeysA, ["apikeys", "list", "service-a"]);
  assert.deepEqual(apiKeysB, ["apikeys", "list", "service-b"]);
  assert.deepEqual(lookupA, ["apikeys", "lookup", "service-a"]);
  assert.deepEqual(lookupB, ["apikeys", "lookup", "service-b"]);
  assert.deepEqual(modelsA, ["managed-models-v2", "selector", "service-a"]);
  assert.deepEqual(modelsB, ["managed-models-v2", "selector", "service-b"]);
  assert.notDeepEqual(apiKeysA, apiKeysB);
  assert.notDeepEqual(lookupA, lookupB);
  assert.notDeepEqual(modelsA, modelsB);
});

test("聚合 API、模型目录和模型组查询键按服务地址隔离", () => {
  const aggregateA = queryKeys.buildAggregateApiListQueryKey("service-a");
  const aggregateB = queryKeys.buildAggregateApiListQueryKey("service-b");
  const modelsA = queryKeys.buildManagedModelListQueryKey("service-a", true);
  const modelsB = queryKeys.buildManagedModelListQueryKey("service-b", true);
  const groupsA = queryKeys.buildModelGroupListQueryKey("service-a");
  const groupsB = queryKeys.buildModelGroupListQueryKey("service-b");

  assert.deepEqual(aggregateA, ["aggregate-apis", "list", "service-a"]);
  assert.deepEqual(aggregateB, ["aggregate-apis", "list", "service-b"]);
  assert.deepEqual(modelsA, ["managed-models-v2", "list", "service-a", true]);
  assert.deepEqual(modelsB, ["managed-models-v2", "list", "service-b", true]);
  assert.deepEqual(groupsA, ["model-groups", "list", "service-a"]);
  assert.deepEqual(groupsB, ["model-groups", "list", "service-b"]);
  assert.notDeepEqual(aggregateA, aggregateB);
  assert.notDeepEqual(modelsA, modelsB);
  assert.notDeepEqual(groupsA, groupsB);
});

test("不同服务的权威用量列表不会互相合并或按旧时间戳抢占", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  const usageAKey = queryKeys.buildAccountUsageListQueryKey("service-a");
  const usageBKey = queryKeys.buildAccountUsageListQueryKey("service-b");
  const usageA = [
    { accountId: "same-id", capturedAt: 200, reserveUsedPercent: 30 },
    { accountId: "only-a", capturedAt: 210, usedPercent: 10 },
  ];
  client.setQueryData(usageAKey, usageA);

  await client.fetchQuery({ queryKey: usageBKey, queryFn: async () => [] });
  assert.deepEqual(client.getQueryData(usageBKey), []);
  assert.deepEqual(client.getQueryData(usageAKey), usageA);

  const firstB = [
    { accountId: "same-id", capturedAt: 100, reserveUsedPercent: 5 },
    { accountId: "removed-next", capturedAt: 90, usedPercent: 20 },
  ];
  await client.fetchQuery({ queryKey: usageBKey, queryFn: async () => firstB });
  assert.deepEqual(client.getQueryData(usageBKey), firstB);

  const nextB = [
    { accountId: "same-id", capturedAt: 80, reserveUsedPercent: 7 },
  ];
  await client.fetchQuery({ queryKey: usageBKey, queryFn: async () => nextB });
  assert.deepEqual(client.getQueryData(usageBKey), nextB);
});

test("同地址请求失败时保留该地址最后一次成功数据", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  const usageKey = queryKeys.buildAccountUsageListQueryKey("service-a");
  const previous = [{ accountId: "account-a", capturedAt: 100 }];
  client.setQueryData(usageKey, previous);

  await assert.rejects(
    client.fetchQuery({
      queryKey: usageKey,
      queryFn: async () => {
        throw new Error("temporary failure");
      },
    }),
    /temporary failure/,
  );
  assert.deepEqual(client.getQueryData(usageKey), previous);
});

test("晚到的旧服务响应只能写入旧服务缓存", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  const usageAKey = queryKeys.buildAccountUsageListQueryKey("service-a");
  const usageBKey = queryKeys.buildAccountUsageListQueryKey("service-b");
  let resolveA;
  const delayedA = new Promise((resolve) => {
    resolveA = resolve;
  });

  const fetchA = client.fetchQuery({
    queryKey: usageAKey,
    queryFn: () => delayedA,
  });
  const usageB = [{ accountId: "account-b", capturedAt: 50 }];
  await client.fetchQuery({ queryKey: usageBKey, queryFn: async () => usageB });
  resolveA([{ accountId: "account-a", capturedAt: 300 }]);
  await fetchA;

  assert.deepEqual(client.getQueryData(usageBKey), usageB);
  assert.deepEqual(client.getQueryData(usageAKey), [
    { accountId: "account-a", capturedAt: 300 },
  ]);
});

test("账号 hook 使用当前地址请求，并直接采用当前成功用量快照", async () => {
  const source = await readSource("src/hooks/useAccounts.ts");

  assert.match(source, /accountClient\.list\(serviceStatus\.addr\)/);
  assert.match(source, /accountClient\.listUsage\(serviceStatus\.addr\)/);
  assert.match(
    source,
    /attachUsagesToAccounts\([\s\S]*visibleAccountList\?\.items \|\| \[\],[\s\S]*usagesQuery\.data \|\| \[\]/,
  );
  assert.doesNotMatch(source, /lastKnownUsagesRef|\.\.\.incomingUsages/);
  assert.doesNotMatch(source, /placeholderData:\s*\(previousData\)/);
});

test("账号页按服务地址重建状态并让模型请求固定到该地址", async () => {
  const source = await readSource("src/app/accounts/page.tsx");

  assert.match(
    source,
    /<AccountsPageContent key=\{serviceAddr \|\| "default"\} serviceAddr=\{serviceAddr\} \/>/,
  );
  assert.match(
    source,
    /accountClient\.fetchAccountModels\(account\.id, serviceAddr\)/,
  );
  assert.match(
    source,
    /accountClient\.associateAccountModels\([\s\S]*displayNames,[\s\S]*serviceAddr/,
  );
});

test("日志页缓存与清空操作只作用于当前服务地址", async () => {
  const source = await readSource("src/app/logs/page.tsx");

  assert.match(source, /buildApiKeyLookupQueryKey\(serviceAddr\)/);
  assert.match(
    source,
    /queryKey:\s*\[\s*"aggregate-apis",\s*"lookup",\s*serviceAddr\s*\]/,
  );
  assert.match(
    source,
    /queryKey:\s*\[\s*"logs",\s*"list-with-summary",\s*serviceAddr,/,
  );
  assert.match(
    source,
    /setQueriesData<RequestLogListWithSummaryResult>\(\s*\{ queryKey: \["logs", "list-with-summary", serviceAddr\] \}/,
  );
  assert.match(source, /clearRequestLogs\(serviceAddr\)/);
  assert.match(source, /listRequestLogsWithSummary\([\s\S]*\{ signal \},\s*serviceAddr,/);
});

test("平台密钥页切换服务时清空敏感状态并隔离模型缓存", async () => {
  const pageSource = await readSource("src/app/apikeys/page.tsx");
  const hookSource = await readSource("src/hooks/useApiKeys.ts");
  const modalSource = await readSource("src/components/modals/api-key-modal.tsx");

  assert.match(pageSource, /setRevealedSecrets\(\{\}\);[\s\S]*\}, \[serviceAddr\]\);/);
  assert.match(pageSource, /<ApiKeyModal\s*key=\{serviceAddr \|\| "default"\}/);
  assert.match(hookSource, /queryKey:\s*apiKeyListQueryKey/);
  assert.match(hookSource, /queryKey:\s*managedModelSelectorQueryKey/);
  assert.match(modalSource, /buildManagedModelSelectorQueryKey\(serviceStatus\.addr\)/);
  assert.match(modalSource, /managedModelsV2Client\.list\(false, serviceStatus\.addr\)/);
});

test("聚合 API、模型组和模型目录的读写固定到当前服务地址", async () => {
  const aggregateSource = await readSource("src/app/aggregate-api/page.tsx");
  const aggregateModalSource = await readSource(
    "src/components/modals/aggregate-api-modal.tsx",
  );
  const modelGroupsSource = await readSource("src/app/model-groups/page.tsx");
  const managedModelsSource = await readSource("src/hooks/useManagedModels.ts");

  assert.match(aggregateSource, /queryKey:\s*aggregateApiListQueryKey/);
  assert.match(aggregateSource, /listAggregateApis\(serviceAddr\)/);
  assert.match(aggregateSource, /deleteAggregateApi\(apiId, serviceAddr\)/);
  assert.match(aggregateSource, /readAggregateApiSecret\(apiId, serviceAddr\)/);
  assert.match(aggregateSource, /fetchAggregateApiModels\(apiId, serviceAddr\)/);
  assert.match(aggregateSource, /key=\{serviceAddr \|\| "default"\}/);
  assert.match(aggregateSource, /serviceAddr=\{serviceAddr\}/);
  assert.match(aggregateModalSource, /serviceAddr:\s*string \| null/);
  assert.match(aggregateModalSource, /\}, serviceAddr\);/);

  assert.match(modelGroupsSource, /queryKey:\s*groupListQueryKey/);
  assert.match(modelGroupsSource, /listModelGroups\(serviceAddr\)/);
  assert.match(modelGroupsSource, /list\(false, serviceAddr\)/);
  assert.match(modelGroupsSource, /deleteModelGroup\(id, serviceAddr\)/);
  assert.match(modelGroupsSource, /\},\s*serviceAddr,\s*\);/);
  assert.match(modelGroupsSource, /\}, \[serviceAddr\]\);/);

  assert.match(managedModelsSource, /queryKey:\s*managedModelsQueryKey/);
  assert.match(managedModelsSource, /list\(true, serviceAddr\)/);
  assert.match(managedModelsSource, /upsert\(input, serviceAddr\)/);
  assert.match(managedModelsSource, /delete\(slug, serviceAddr\)/);
  assert.match(managedModelsSource, /previewImport\(input, serviceAddr\)/);
  assert.match(managedModelsSource, /commitImport\(input, serviceAddr\)/);
});

test("账号表格选择框有可访问名称且操作栏不伪装成残缺表格", async () => {
  const source = await readSource("src/app/accounts/accounts-page-view.tsx");

  assert.match(source, /aria-label=\{t\("全选"\)\}/);
  assert.match(source, /aria-label=\{`\$\{t\("选择账号"\)\} \$\{account\.name\}`\}/);
  assert.match(source, /className="account-pool-action-rail"\s*role="group"/);
  assert.doesNotMatch(source, /role="(?:columnheader|cell)"/);
});
