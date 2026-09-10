import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import test from "node:test";

// 约束操作入口与删除范围，避免后续样式调整恢复三点按钮或绕过确认直接删除。
test("会话操作使用右键菜单和带确认的删除按钮", async () => {
  const source = await readFile(new URL("../src/app/sessions/page.tsx", import.meta.url), "utf8");
  assert.match(source, /ContextMenu\.Trigger/);
  assert.doesNotMatch(source, /MoreHorizontal|DropdownMenuTrigger/);
  assert.match(source, /mutationFn: sessionRoutingClient\.reset/);
  assert.match(source, /confirmVariant="destructive"/);
  assert.match(source, /删除会话记录/);
  assert.match(source, /setPage\(page - 1\)/);
  assert.match(source, /不会删除聊天内容/);
});
