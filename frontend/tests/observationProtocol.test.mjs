import assert from "node:assert/strict";
import { readFile, writeFile, unlink } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import ts from "../node_modules/typescript/lib/typescript.js";

// 编译真实业务纯函数验证所有协议别名；动态模块只创建一个临时文件，执行后严格删除。
test("直连 WebSocket、预热与网关传输标签一致", async () => {
  const source = await readFile(new URL("../src/lib/utils/requestProtocol.ts", import.meta.url), "utf8");
  const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 } });
  const modulePath = join(tmpdir(), `observationProtocol${randomUUID()}.mjs`);
  await writeFile(modulePath, compiled.outputText, { flag: "wx" });
  try {
    const { normalizeRequestType } = await import(pathToFileURL(modulePath).href);
    for (const value of ["ws", "websocket", " WebSocket ", "websocketHandshake"]) assert.equal(normalizeRequestType(value), "ws");
    for (const value of ["websocketPrewarm", "wsPrewarm"]) assert.equal(normalizeRequestType(value), "wsPrewarm");
    assert.equal(normalizeRequestType("clientResponse"), "client");
    // 两层组件重复调用时保持同一类型，尤其不能把客户端完成事件变成 HTTP。
    for (const value of ["clientResponse", "client", "websocket", "wsPrewarm", "http"]) {
      const normalized = normalizeRequestType(value);
      assert.equal(normalizeRequestType(normalized), normalized);
    }
    for (const value of ["http", "sse", ""]) assert.equal(normalizeRequestType(value), "http");
  } finally {
    await unlink(modulePath);
  }
});
