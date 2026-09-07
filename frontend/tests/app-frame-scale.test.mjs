import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const appFramePath = path.join(
  testDir,
  "..",
  "src",
  "components",
  "layout",
  "app-frame.tsx",
);

test("主内容区保持普通布局，浮层和 sticky 使用同一坐标系", async () => {
  const source = await fs.readFile(appFramePath, "utf8");

  assert.match(source, /data-slot="app-main-scale"/);
  assert.doesNotMatch(source, /scale-90|111\.111111|origin-top-left/);
  assert.match(source, /h-dvh/);
  assert.match(source, /data-slot="app-content"/);
  assert.match(source, /min-h-0 min-w-0 flex-1 overflow-y-auto/);
});
