import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "utils", "request.ts");

async function loadRequestModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "codexmanager-request-"));
  const tempFile = path.join(tempDir, "request.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const request = await loadRequestModule();

test("fetchWithRetry rejects before fetch when parent signal is already aborted", async () => {
  const controller = new AbortController();
  controller.abort();
  let called = false;
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => {
    called = true;
    return new Response(null, { status: 200 });
  };

  try {
    await assert.rejects(
      request.fetchWithRetry("https://example.test/rpc", {}, { signal: controller.signal }),
      { name: "AbortError" },
    );
    assert.equal(called, false);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("runWithControl aborts retry delay without starting another attempt", async () => {
  const controller = new AbortController();
  let attempts = 0;
  const pending = request.runWithControl(
    async () => {
      attempts += 1;
      throw new Error("temporary failure");
    },
    {
      retries: 2,
      retryDelayMs: 1_000,
      signal: controller.signal,
    },
  );

  setTimeout(() => controller.abort(), 10);
  await assert.rejects(pending, { name: "AbortError" });
  assert.equal(attempts, 1);
});
