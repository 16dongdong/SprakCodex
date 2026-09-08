import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";

const page = await fs.readFile(new URL("../src/app/page.tsx", import.meta.url), "utf8");
const panels = await fs.readFile(new URL("../src/components/dashboard/overviewPanels.tsx", import.meta.url), "utf8");

// 回归检查首页查询边界，防止恢复仪表盘时重新加载平台功能或在后台持续轮询。
test("个人仪表盘使用当天真实汇总并限制后台刷新和列表长度", () => {
  assert.match(page, /enabled: service.connected && active/);
  assert.match(page, /refetchInterval: active \? refreshIntervalMs : false/);
  assert.match(page, /dayStartTs, dayEndTs/);
  assert.doesNotMatch(page, /listRequestLogsWithSummary/);
  assert.match(page, /includeAccounts: false, includeUsageSnapshots: false/);
  assert.doesNotMatch(page, /navigateShellPath\("\/accounts"\)/);
  assert.match(page, /role="alert"/);
});

// 单条请求使用单条 Token 标题，缺失额度和空列表必须具有独立的展示语义。
test("仪表盘区分未知额度、空日志和单次请求指标", () => {
  assert.match(panels, /Number.isFinite\(value\)/);
  assert.match(panels, /暂无请求日志/);
  assert.match(panels, /AreaChart/);
  assert.doesNotMatch(panels, /<table/);
  assert.doesNotMatch(panels, /今日Token/);
  assert.match(panels, /active && points.length/);
});
