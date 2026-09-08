"use client";

import { formatCompactTokenAmount } from "@/lib/dashboard/format";
import { useQuery } from "@tanstack/react-query";
import { Activity, ArrowUpRight, Coins, Layers3, RefreshCw, Users } from "lucide-react";
import { DashboardQuota, DashboardRequests } from "@/components/dashboard/overviewPanels";
import { Button } from "@/components/ui/button";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useLocalDayRange } from "@/hooks/useLocalDayRange";
import { serviceClient } from "@/lib/api/service-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";

const refreshIntervalMs = 15_000;
const recentRequestLimit = 100;

// 首页只读取个人统计；缓存页隐藏或服务断开时停止轮询，失败显示错误而不是伪造零用量。
export default function HomePage() {
  const { t } = useI18n();
  const service = useAppStore((state) => state.serviceStatus);
  const navigate = useAppStore((state) => state.navigateShellPath);
  const active = useDesktopPageActive("/");
  const { dayStartTs, dayEndTs } = useLocalDayRange();
  const overview = useQuery({
    queryKey: ["personalDashboard", service.addr, dayStartTs, dayEndTs],
    enabled: service.connected && active,
    refetchInterval: active ? refreshIntervalMs : false,
    // 统计使用本地日界限，最近列表只取固定条数；不加载平台密钥、模型或完整账号详情。
    queryFn: async () => {
      const [snapshot, requests] = await Promise.all([
        serviceClient.getStartupSnapshot({
          requestLogLimit: 0, dayStartTs, dayEndTs,
          includeApiModels: false, includeApiKeys: false,
          includeAccountRuntime: false, includeAccountDetails: false,
          includeAccounts: false, includeUsageSnapshots: false,
        }),
        serviceClient.listRequestLogsWithSummary({
          page: 1, pageSize: recentRequestLimit, startTs: dayStartTs, endTs: dayEndTs,
        }),
      ]);
      return { snapshot, requests };
    },
  });
  const snapshot = overview.data?.snapshot;
  const requests = overview.data?.requests;
  const usage = snapshot?.requestLogTodaySummary;
  const metrics = [
    { label: "今日Token", value: formatCompactTokenAmount(usage?.todayTokens), icon: Layers3, detail: "输入 + 输出合计" },
    { label: "预计费用", value: usage ? `$${usage.estimatedCost.toFixed(4)}` : undefined, icon: Coins, detail: "按本地价格表估算，非实际账单" },
    { label: "成功请求", value: requests?.summary.successCount, icon: Activity, detail: "成功请求" },
    { label: "可用账号", value: snapshot ? `${snapshot.accountSummary.availableCount} / ${snapshot.accountSummary.accountCount}` : undefined, icon: Users, detail: "当前健康可调用的账号" },
  ];

  return (
    <div className="mx-auto max-w-[1440px] space-y-6 pb-6" data-testid="personal-dashboard">
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="mb-2 flex items-center gap-2 text-xs text-muted-foreground">
            <span className={`size-2 rounded-full ${service.connected ? "bg-emerald-500" : "bg-muted-foreground"}`} />
            {t(service.connected ? "服务已连接" : "正在等待服务连接")}
            <span aria-hidden="true">·</span>
            <span>{new Date(dayStartTs * 1000).toLocaleDateString()}</span>
          </div>
          <h1 className="text-3xl font-semibold tracking-tight">{t("仪表盘")}</h1>
        </div>
        <Button variant="outline" className="rounded-xl" disabled={!service.connected || overview.isFetching} onClick={() => void overview.refetch()}>
          <RefreshCw className={`mr-2 size-4 ${overview.isFetching ? "animate-spin motion-reduce:animate-none" : ""}`} />{t("刷新")}
        </Button>
      </header>
      {overview.error && <p role="alert" className="rounded-xl border border-destructive/25 bg-destructive/5 p-4 text-sm text-destructive">{getAppErrorMessage(overview.error)}</p>}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map(({ label, value, icon: Icon, detail }, index) => (
          <section key={label} className={`glass-card relative overflow-hidden rounded-2xl border p-5 ${index === 0 ? "border-primary/30" : "border-border/60"}`}>
            <div className="flex items-center justify-between gap-3">
              <h2 className="text-sm font-medium text-muted-foreground">{t(label)}</h2>
              <span className="rounded-xl bg-primary/8 p-2 text-primary"><Icon className="size-4" /></span>
            </div>
            <p className="mt-5 truncate text-3xl font-semibold tracking-tight tabular-nums" title={String(value ?? "—")}>{typeof value === "number" ? value.toLocaleString() : value ?? "—"}</p>
            <p className="mt-3 text-xs text-muted-foreground">{index === 2 ? `${t("异常请求")} · ${requests?.summary.errorCount ?? "—"}` : t(detail)}</p>
          </section>
        ))}
      </div>
      <div className="grid gap-4 lg:grid-cols-[1.15fr_1fr]">
        <section className="glass-card rounded-2xl border border-border/60 p-5 sm:p-6">
          <h2 className="font-semibold">{t("今日Token")}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t("包含每次请求的历史上下文；缓存属于输入，推理属于输出，不重复相加。")}</p>
          <div className="mt-6 grid grid-cols-2 gap-6">
            {([["非缓存输入", usage ? Math.max(0, usage.inputTokens - usage.cachedInputTokens) : undefined], ["缓存Token", usage?.cachedInputTokens], ["输出 Token", usage?.outputTokens], ["推理Token", usage?.reasoningOutputTokens]] as const).map(([label, value]) => (
              <div key={label} className="border-l-2 border-primary/30 pl-4">
                <p className="text-xs text-muted-foreground">{t(label)}</p>
                <p className="mt-2 text-xl font-semibold tabular-nums">{formatCompactTokenAmount(value)}</p>
              </div>
            ))}
          </div>
        </section>
        <section className="glass-card rounded-2xl border border-border/60 p-5 sm:p-6">
          <div className="flex items-center justify-between gap-3">
            <h2 className="font-semibold">{t("账号池剩余")}</h2>
            <Button variant="ghost" size="sm" onClick={() => navigate("/accounts")}>{t("账号")}<ArrowUpRight className="ml-1 size-4" /></Button>
          </div>
          <div className="mt-4 space-y-4">
            <DashboardQuota label={t("5小时剩余")} value={snapshot?.accountSummary.primaryRemainPercent} />
            <DashboardQuota label={t("7天剩余")} value={snapshot?.accountSummary.secondaryRemainPercent} />
          </div>
        </section>
      </div>
      <DashboardRequests requests={requests} loading={overview.isPending && service.connected} />
    </div>
  );
}
