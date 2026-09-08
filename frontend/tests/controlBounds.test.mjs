import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";

// 共享控件禁止透明伪元素扩大命中范围；验证焦点样式仍在，避免修复鼠标误触时破坏键盘操作。
test("开关和复选框没有向外扩张的透明点击层", async () => {
  for (const name of ["switch", "checkbox"]) {
    const source = await fs.readFile(new URL(`../src/components/ui/${name}.tsx`, import.meta.url), "utf8");
    assert.doesNotMatch(source, /(?:after|before):-inset/);
    assert.match(source, /focus-visible:ring/);
  }
});
