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
  assert.match(source, /xl:scale-90/);
  assert.match(source, /xl:h-\[111\.111111%\]/);
  assert.match(source, /xl:w-\[111\.111111%\]/);
  assert.match(source, /origin-top-left/);
  assert.match(source, /px-4 pb-7 pt-4/);
  assert.match(source, /lg:px-5 lg:pt-5/);
  assert.match(source, /xl:pl-\[26px\]/);
  assert.match(source, /xl:pt-\[26px\]/);
});
