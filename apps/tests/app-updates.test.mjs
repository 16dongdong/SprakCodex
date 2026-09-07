import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "api", "app-updates.ts");

async function loadAppUpdatesModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-app-updates-")
  );
  const tempFile = path.join(tempDir, "app-updates.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const appUpdates = await loadAppUpdatesModule();

test("readUpdateCheckResult 解析更新检查结果并保留可空字段", () => {
  const result = appUpdates.readUpdateCheckResult({
    repo: "team/project",
    mode: "stable",
    isPortable: true,
    hasUpdate: true,
    canPrepare: true,
    currentVersion: "1.0.0",
    latestVersion: "1.1.0",
    releaseTag: "v1.1.0",
    releaseName: "Spring",
    publishedAt: "2026-04-13T10:00:00Z",
    checkedAtUnixSecs: "123456",
  });

  assert.equal(result.repo, "team/project");
  assert.equal(result.mode, "stable");
  assert.equal(result.isPortable, true);
  assert.equal(result.hasUpdate, true);
  assert.equal(result.canPrepare, true);
  assert.equal(result.currentVersion, "1.0.0");
  assert.equal(result.latestVersion, "1.1.0");
  assert.equal(result.releaseTag, "v1.1.0");
  assert.equal(result.releaseName, "Spring");
  assert.equal(result.publishedAt, "2026-04-13T10:00:00Z");
  assert.equal(result.reason, null);
  assert.equal(result.checkedAtUnixSecs, 123456);
});

test("readUpdatePrepareResult 与 readUpdateActionResult 统一补齐结果", () => {
  const prepare = appUpdates.readUpdatePrepareResult({
    prepared: true,
    mode: "stable",
    isPortable: false,
    releaseTag: "v1.1.0",
    latestVersion: "1.1.0",
    assetName: "CodexManager-Setup.exe",
    assetPath: "C:/updates/setup.exe",
    downloaded: true,
  });
  assert.equal(prepare.prepared, true);
  assert.equal(prepare.isPortable, false);
  assert.equal(prepare.assetName, "CodexManager-Setup.exe");

  const action = appUpdates.readUpdateActionResult({
    ok: true,
    message: "ready",
  });
  assert.equal(action.ok, true);
  assert.equal(action.message, "ready");
});

test("readUpdateStatusResult 统一解析 pending 与 lastCheck", () => {
  const status = appUpdates.readUpdateStatusResult({
    repo: "team/project",
    mode: "stable",
    isPortable: true,
    currentVersion: "1.0.0",
    currentExePath: "C:/CodexManager.exe",
    portableMarkerPath: "C:/portable.flag",
    pending: {
      mode: "stable",
      isPortable: true,
      releaseTag: "v1.1.0",
      latestVersion: "1.1.0",
      assetName: "CodexManager.zip",
      assetPath: "C:/updates/CodexManager.zip",
      installerPath: null,
      stagingDir: "C:/updates/staging",
      preparedAtUnixSecs: 999,
    },
    lastCheck: {
      hasUpdate: true,
      latestVersion: "1.1.0",
      releaseTag: "v1.1.0",
    },
    lastError: "network",
  });

  assert.equal(status.repo, "team/project");
  assert.equal(status.pending?.prepared, true);
  assert.equal(status.pending?.downloaded, true);
  assert.equal(status.pending?.stagingDir, "C:/updates/staging");
  assert.equal(status.pending?.preparedAtUnixSecs, 999);
  assert.equal(status.lastCheck?.hasUpdate, true);
  assert.equal(status.lastCheck?.latestVersion, "1.1.0");
  assert.equal(status.lastError, "network");
});
