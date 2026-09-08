import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";
import ts from "../node_modules/typescript/lib/typescript.js";

const source = await fs.readFile(new URL("../src/lib/dashboard/format.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ES2022 } }).outputText;
const { formatCompactTokenAmount } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);

// 用边界向量验证统一单位，未知值不得显示为零，舍入后不得出现 1000K。
test("Token 格式覆盖 K/M/B、未知值与跨档舍入", () => {
  for (const [input, expected] of [[null, "—"], [NaN, "—"], [0, "0"], [999, "999"], [1000, "1K"], [213176, "213.18K"], [1000000, "1M"], [1000000000, "1B"], [999999, "1M"]]) {
    assert.equal(formatCompactTokenAmount(input), expected);
  }
});

// 详情标签与纯文本展示边界避免退回四区文本框，也避免把网络正文当 HTML 执行。
test("Network 详情具有独立标签、搜索、复制和纯文本渲染", async () => {
  const dialog = await fs.readFile(new URL("../src/components/modals/requestDetailsDialog.tsx", import.meta.url), "utf8");
  const viewer = await fs.readFile(new URL("../src/components/modals/networkDetailViews.tsx", import.meta.url), "utf8");
  for (const tab of ["headers", "payload", "response", "events"]) assert.ok(dialog.includes(`value="${tab}"`));
  assert.match(viewer, /navigator.clipboard.writeText\(text\)/);
  assert.match(viewer, /JSON.parse\(value\)/);
  assert.doesNotMatch(viewer, /dangerouslySetInnerHTML/);
});
