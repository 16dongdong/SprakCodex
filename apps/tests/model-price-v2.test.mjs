import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import ts from "../node_modules/typescript/lib/typescript.js";

const sourcePath = path.resolve(
  import.meta.dirname,
  "../src/lib/api/model-price-v2.ts",
);
const source = await fs.readFile(sourcePath, "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  },
  fileName: sourcePath,
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled.outputText).toString("base64")}`;
const {
  microusdToUsdPerMillion,
  usdPerMillionToMicrousd,
} = await import(moduleUrl);

test("USD/1M 价格无损转换为整数 micro-USD", () => {
  assert.equal(usdPerMillionToMicrousd("2.5"), 2_500_000);
  assert.equal(usdPerMillionToMicrousd("0.000001"), 1);
  assert.equal(usdPerMillionToMicrousd("0.0000010"), 1);
  assert.equal(usdPerMillionToMicrousd(" 12 "), 12_000_000);
});

test("USD/1M 价格拒绝精度丢失和非法输入", () => {
  assert.throws(() => usdPerMillionToMicrousd("0.0000001"), /6 位有效小数/);
  assert.throws(() => usdPerMillionToMicrousd("-1"), /非负十进制数/);
  assert.throws(() => usdPerMillionToMicrousd("not-a-number"), /非负十进制数/);
  assert.throws(() => usdPerMillionToMicrousd("9007199255"), /安全整数范围/);
});

test("整数 micro-USD 可稳定格式化为 USD/1M", () => {
  assert.equal(microusdToUsdPerMillion(null), "");
  assert.equal(microusdToUsdPerMillion(2_500_000), "2.5");
  assert.equal(microusdToUsdPerMillion(1), "0.000001");
  assert.throws(() => microusdToUsdPerMillion(-1), /无效/);
  assert.throws(
    () => microusdToUsdPerMillion(Number.MAX_SAFE_INTEGER + 1),
    /无效/,
  );
});
