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
  "api-key-update-payload.ts",
);

async function loadPayloadModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-api-key-update-payload-"),
  );
  const tempFile = path.join(tempDir, "api-key-update-payload.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const payloadModule = await loadPayloadModule();

test("API key group filter payload distinguishes omission, value, and explicit null", () => {
  const omitted = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    name: "Renamed key",
  });
  assert.equal("accountGroupFilter" in omitted, false);
  assert.equal("hasAccountGroupFilter" in omitted, false);

  const updated = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    accountGroupFilter: "team-a",
  });
  assert.equal(updated.hasAccountGroupFilter, true);
  assert.equal(updated.accountGroupFilter, "team-a");

  const cleared = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    accountGroupFilter: null,
  });
  assert.equal(cleared.hasAccountGroupFilter, true);
  assert.equal(cleared.accountGroupFilter, null);
});

test("partial API key updates do not synthesize name, model, or routing clears", () => {
  const groupOnly = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    accountGroupFilter: "team-a",
  });
  for (const field of [
    "name",
    "modelSlug",
    "reasoningEffort",
    "serviceTier",
    "rotationStrategy",
    "aggregateApiId",
    "accountPlanFilter",
  ]) {
    assert.equal(field in groupOnly, false, `${field} must remain omitted`);
  }
  assert.equal("hasName" in groupOnly, false);
  assert.equal("hasModelConfig" in groupOnly, false);
  assert.equal("hasRoutingConfig" in groupOnly, false);

  const nameOnly = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    name: "Renamed key",
  });
  assert.equal(nameOnly.hasName, true);
  assert.equal(nameOnly.name, "Renamed key");
  assert.equal("modelSlug" in nameOnly, false);
  assert.equal("rotationStrategy" in nameOnly, false);

  const fullGroups = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    modelSlug: null,
    reasoningEffort: "high",
    serviceTier: null,
    rotationStrategy: "hybrid_rotation",
    aggregateApiId: null,
    accountPlanFilter: "plus",
  });
  assert.equal(fullGroups.hasModelConfig, true);
  assert.equal(fullGroups.modelSlug, null);
  assert.equal(fullGroups.reasoningEffort, "high");
  assert.equal(fullGroups.serviceTier, null);
  assert.equal(fullGroups.hasRoutingConfig, true);
  assert.equal(fullGroups.rotationStrategy, "hybrid_rotation");
  assert.equal(fullGroups.aggregateApiId, null);
  assert.equal(fullGroups.accountPlanFilter, "plus");
});

test("API key quota payload keeps the existing three-state presence contract", () => {
  const omitted = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {});
  assert.equal("quotaLimitTokens" in omitted, false);
  assert.equal("hasQuotaLimitTokens" in omitted, false);

  const updated = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    quotaLimitTokens: 123,
  });
  assert.equal(updated.hasQuotaLimitTokens, true);
  assert.equal(updated.quotaLimitTokens, 123);

  const cleared = payloadModule.buildApiKeyUpdateInvokePayload("key-1", {
    quotaLimitTokens: null,
  });
  assert.equal(cleared.hasQuotaLimitTokens, true);
  assert.equal(cleared.quotaLimitTokens, null);
});
