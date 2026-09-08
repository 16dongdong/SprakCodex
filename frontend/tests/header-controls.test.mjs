import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const headerPath = path.join(
  testDir,
  "..",
  "src",
  "components",
  "layout",
  "header.tsx",
);

// 顶栏不再拥有操作入口，迁移后设置页负责语言和免责声明。
test("顶部仅保留标题，设置承接语言与声明", async () => {
  const source = await fs.readFile(headerPath, "utf8");
  const settings = await fs.readFile(path.join(testDir, "..", "src", "app", "settings", "page.tsx"), "utf8");
  assert.doesNotMatch(source, /LanguageSwitcher|DisclaimerTicker|serviceClient|<Switch/);
  assert.match(settings, /<LanguageSwitcher/);
  assert.match(settings, /<DisclaimerTicker/);
  assert.match(source, /getTopLevelRouteLabel/);
});
