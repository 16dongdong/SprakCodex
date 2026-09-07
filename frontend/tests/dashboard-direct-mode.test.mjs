import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

// 接入说明必须跟随真实观测能力，不能在另一页面继续宣称直连不记录；生命周期提示明确区分退出与停用。
test("直连接入说明与完成事件恢复行为一致", async () => {
  const sections = await fs.readFile(path.join(appsRoot, "src/app/platform-mode/page-sections.tsx"), "utf8");
  const panel = await fs.readFile(path.join(appsRoot, "src/components/settings/observationPanel.tsx"), "utf8");
  assert.doesNotMatch(sections, /CodexManager 不记录|不会产生 CodexManager 请求日志|仪表盘用量统计不可用/);
  assert.match(sections, /启用观测后可记录/);
  assert.match(panel, /本机观测入口：/);
  assert.match(panel, /退出应用不等于关闭观测/);
});

// Windows 子进程由原生扫描器接入；禁止重新把宿主临时端口固化进终端代理环境。
test("Windows 项目启动不再覆盖原代理或 CA 环境", async () => {
  const source = await fs.readFile(path.join(appsRoot, "src-tauri/src/commands/codex_projects.rs"), "utf8");
  const start = source.indexOf("fn launch_codex_terminal(");
  const end = source.indexOf("fn macos_terminal_script(", start);
  assert.ok(start >= 0 && end > start);
  const launcher = source.slice(start, end);
  assert.doesNotMatch(launcher, /configureChild|HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|CODEX_CA_CERTIFICATE|SSL_CERT_FILE/);
  assert.match(launcher, /spawn_and_reap/);
});

async function readDashboardSource() {
  return fs.readFile(path.join(appsRoot, "src/app/page.tsx"), "utf8");
}

async function readSource(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

// 官方直连观测的记录也进入用量分析；验收不得回退到旧的“切换网关才可统计”行为。
test("账号直连模式说明观测口径并保持用量分析可见", async () => {
  const source = await readDashboardSource();
  const gatewayStatusSource = await readSource("src/components/dashboard/dashboard-gateway-status.tsx");
  assert.match(source, /useCodexProfileModeStatus/);
  assert.doesNotMatch(source, /DirectModeUnavailable/);
  assert.doesNotMatch(source, /账号直连模式下不可用/);
  assert.match(gatewayStatusSource, /当前为账号直连模式/);
  assert.match(gatewayStatusSource, /保留 Codex 原有登录与上游/);
  assert.match(gatewayStatusSource, /不扣除平台钱包或密钥额度/);
  assert.match(source, /<AdminUsageAnalyticsCard/);
  assert.doesNotMatch(source, /当前活跃账号/);
  assert.doesNotMatch(source, /智能推荐/);
});

// 用户要求彻底移除直连提示与跳转入口；连同只服务于提示条的轮询和 props 一并检查。
test("日志页不保留直连提示条、跳转网关按钮或模式轮询", async () => {
  const source = await readSource("src/app/logs/page.tsx");
  const sections = await readSource("src/app/logs/page-sections.tsx");
  assert.doesNotMatch(source, /useCodexProfileModeStatus|isDirectAccountMode/);
  assert.doesNotMatch(sections, /isDirectAccountMode|去切换为本地网关|账号直连模式不会产生/);
  assert.doesNotMatch(source, /DirectModeUnavailable/);
});

// 删除的提示不应藏在其他语言资源中，避免后续引用旧 key 又把引导条或遮罩带回页面。
test("所有语言移除直连限制提示的弃用文案", async () => {
  const obsoleteCopy = /账号直连模式不会产生新的 CodexManager 请求日志|这里仅展示历史网关请求|去切换为本地网关|仅网关流量|账号直连模式下不会产生请求日志|账号直连模式下不可用|切换到本地网关后可统计请求日志、Token 和费用|CodexManager 无法统计 CLI 请求日志和用量。/;
  for (const language of ["en", "ko", "ru"]) {
    const common = await readSource(`src/lib/i18n/messages/${language}.ts`);
    const dashboard = await readSource(`src/lib/i18n/messages/sections/${language}-dashboard.ts`);
    assert.doesNotMatch(common, obsoleteCopy);
    assert.doesNotMatch(dashboard, obsoleteCopy);
  }
});

test("启动快照只预取轻量日志样本", async () => {
  const source = await readSource("src/lib/api/startup-snapshot.ts");
  assert.match(source, /STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT = 24/);
});

test("启动快照缓存键包含完整日期边界", async () => {
  const startupSource = await readSource("src/lib/api/startup-snapshot.ts");
  assert.match(startupSource, /dayStartTs \|\| null,\s*dayEndTs \|\| null,/s);

  const dashboardSource = await readSource("src/hooks/useDashboardStats.ts");
  assert.match(
    dashboardSource,
    /buildStartupSnapshotQueryKey\(\s*serviceStatus\.addr,\s*requestLogLimit,\s*localDayRange\.dayStartTs,\s*localDayRange\.dayEndTs,\s*includeApiModels,\s*includeApiKeys,\s*includeAccounts,\s*includeUsageSnapshots,\s*includeAccountRuntime,\s*includeAccountDetails,/s,
  );
});

test("仪表盘只预取状态块所需的轻量数据", async () => {
  const source = await readDashboardSource();
  assert.match(source, /useDashboardStats\(\{\s*requestLogLimit: 0,\s*includeAccountHints: false,/s);
  assert.match(
    source,
    /includeApiModels: false,\s*includeApiKeys: false,\s*includeAccounts: true,\s*includeUsageSnapshots: true,\s*includeAccountRuntime: true,\s*includeAccountDetails: false,/s,
  );
});

test("桌面启动快照命令会透传轻量快照参数", async () => {
  const source = await readSource("src-tauri/src/commands/startup.rs");
  assert.match(source, /day_start_ts: Option<i64>/);
  assert.match(source, /day_end_ts: Option<i64>/);
  assert.match(source, /include_api_models: Option<bool>/);
  assert.match(source, /include_api_keys: Option<bool>/);
  assert.match(source, /include_accounts: Option<bool>/);
  assert.match(source, /include_usage_snapshots: Option<bool>/);
  assert.match(source, /include_account_runtime: Option<bool>/);
  assert.match(source, /include_account_details: Option<bool>/);
  assert.match(source, /"includeApiModels": include_api_models/);
  assert.match(source, /"includeApiKeys": include_api_keys/);
  assert.match(source, /"includeAccounts": include_accounts/);
  assert.match(source, /"includeUsageSnapshots": include_usage_snapshots/);
  assert.match(source, /"includeAccountRuntime": include_account_runtime/);
  assert.match(source, /"includeAccountDetails": include_account_details/);
  assert.match(
    source,
    /rpc_call_in_background\("startup\/snapshot", addr, Some\(params\)\)\.await/,
  );
});

test("托盘预览使用较轻的启动快照", async () => {
  const source = await readSource("src/app/tray-preview/page.tsx");
  assert.match(source, /requestLogLimit: TRAY_PREVIEW_REQUEST_LOG_LIMIT/);
  assert.match(source, /includeApiModels: false/);
  assert.match(source, /includeApiKeys: false/);
  assert.match(source, /includeAccountDetails: false/);
});

test("首页账户统计优先使用启动快照汇总", async () => {
  const hookSource = await readSource("src/hooks/useDashboardStats.ts");
  assert.match(hookSource, /const accountSummary = data\?\.accountSummary;/);
  assert.match(
    hookSource,
    /const totalAccounts = accountSummary\?\.accountCount \?\? accounts\.length;/,
  );
  assert.match(
    hookSource,
    /accountSummary\?\.availableCount \?\? accounts\.filter\(\(item\) => item\.isAvailable\)\.length;/,
  );

  const normalizeSource = await readSource("src/lib/api/normalize.ts");
  assert.match(normalizeSource, /function normalizeStartupAccountSummary/);
  assert.match(normalizeSource, /accountSummary: normalizeStartupAccountSummary/);
});

test("成员用量趋势卡不再重复展示 Top Key", async () => {
  const source = await readDashboardSource();
  const trendCard = source.slice(
    source.indexOf("function MemberUsageTrendCard"),
    source.indexOf("function TopUsageList"),
  );
  assert.match(trendCard, /title=\{t\("Top 模型"\)\}/);
  assert.doesNotMatch(trendCard, /title=\{t\("Top Key"\)\}/);
});
