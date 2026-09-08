import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";
import ts from "../node_modules/typescript/lib/typescript.js";
const source = await fs.readFile(new URL("../src/lib/dashboard/chartRange.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ES2022 } }).outputText;
const { getChartRange } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
// 本地日历边界覆盖跨年、周日及闰月，避免标签范围按固定天数漂移。
test("曲线日周月标签采用正确的本地日历边界", () => {
  for (const [period, date, start, end] of [
    ["day", [2026, 8, 8], [2026, 8, 8], [2026, 8, 9]],
    ["week", [2026, 0, 4], [2025, 11, 29], [2026, 0, 5]],
    ["month", [2024, 1, 29], [2024, 1, 1], [2024, 2, 1]],
    ["month", [2026, 11, 31], [2026, 11, 1], [2027, 0, 1]],
  ]) {
    assert.deepEqual(getChartRange(period, new Date(...date).getTime() / 1000), { startTs: new Date(...start).getTime() / 1000, endTs: new Date(...end).getTime() / 1000 });
  }
});
