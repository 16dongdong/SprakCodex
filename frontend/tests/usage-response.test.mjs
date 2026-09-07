import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "api", "usage-response.ts");

async function loadUsageResponseModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-usage-response-")
  );
  const tempFile = path.join(tempDir, "usage-response.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const usageResponse = await loadUsageResponseModule();

test("unwrapUsageSnapshotPayload 优先解出 snapshot envelope", () => {
  const snapshot = { total: 10 };
  assert.deepEqual(
    usageResponse.unwrapUsageSnapshotPayload({ snapshot }),
    snapshot
  );
});

test("unwrapUsageSnapshotPayload 在无 envelope 时保留原始 payload", () => {
  const payload = { total: 20 };
  assert.deepEqual(usageResponse.unwrapUsageSnapshotPayload(payload), payload);
  assert.equal(usageResponse.unwrapUsageSnapshotPayload(null), null);
});
