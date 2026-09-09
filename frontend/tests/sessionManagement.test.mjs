import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";
import ts from "../node_modules/typescript/lib/typescript.js";
const source=await fs.readFile(new URL("../src/lib/utils/quotaPalette.ts",import.meta.url),"utf8");
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ES2022}}).outputText;
const {quotaPalette}=await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
// 统一额度阈值包含两端，未知额度保持灰色，避免被当作真实零额度。
test("额度颜色按剩余比例区分并覆盖边界",()=>{
  for(const value of [null,undefined,NaN,-1]) assert.equal(quotaPalette(value).indicator,"bg-zinc-400");
  for(const value of [0,29.99]) assert.equal(quotaPalette(value).indicator,"bg-red-500");
  for(const value of [30,45,60]) assert.equal(quotaPalette(value).indicator,"bg-yellow-500");
  for(const value of [60.01,100]) assert.equal(quotaPalette(value).indicator,"bg-green-500");
});
// 会话管理不删除聊天内容，写操作通过统一传输并保留用户确认。
test("会话管理提供重置和手动换号且不直接读取聊天目录",async()=>{
  const page=await fs.readFile(new URL("../src/app/sessions/page.tsx",import.meta.url),"utf8");
  assert.match(page, /ConfirmDialog/);
  assert.match(page, /switchAccount/);
  assert.match(page, /sessionRoutingClient.reset/);
  assert.match(page, /useDesktopPageActive\("\/sessions"\)/);
  assert.doesNotMatch(page, /\bfetch\(|auth\.json|\.codex/);
});
