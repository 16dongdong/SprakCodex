import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "utils", "reset-credits.ts");

async function loadModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "codexmanager-reset-credits-"));
  const tempFile = path.join(tempDir, "reset-credits.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const { isResetCreditAvailable, readCachedResetCredits } = await loadModule();

function usage(creditsJson) {
  return { creditsJson };
}

test("readCachedResetCredits distinguishes an absent field from a pending count", () => {
  assert.deepEqual(readCachedResetCredits(usage('{"balance": 1}')), {
    present: false,
    availableCount: null,
  });
  assert.deepEqual(
    readCachedResetCredits(usage('{"rate_limit_reset_credits": {}}')),
    { present: true, availableCount: null },
  );
});

test("readCachedResetCredits normalizes the cached available count", () => {
  assert.deepEqual(
    readCachedResetCredits(
      usage('{"rate_limit_reset_credits": {"available_count": "2.9"}}'),
    ),
    { present: true, availableCount: 2 },
  );
  assert.deepEqual(readCachedResetCredits(usage("not-json")), {
    present: false,
    availableCount: null,
  });
});

test("isResetCreditAvailable accepts only explicit, unexpired available records", () => {
  const future = 2_000;
  assert.equal(
    isResetCreditAvailable({ status: "available", rawStatus: null, expiresAt: future }, 1_000),
    true,
  );
  assert.equal(
    isResetCreditAvailable({ status: "pending", rawStatus: null, expiresAt: future }, 1_000),
    false,
  );
  assert.equal(
    isResetCreditAvailable({ status: null, rawStatus: null, expiresAt: future }, 1_000),
    false,
  );
  assert.equal(
    isResetCreditAvailable({ status: "available", rawStatus: null, expiresAt: 999 }, 1_000),
    false,
  );
});
