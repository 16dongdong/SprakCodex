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

// 页面标题由顶栏统一提供，内容区仅保留状态和刷新操作；首页只读取个人统计；缓存页隐藏或服务断开时停止轮询，失败显示错误而不是伪造零用量。
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
    // 当日明细与累计卡片各自使用对应统计口径；曲线独立查询，不额外下载请求列表。
    queryFn: async () => {
      const [snapshot, cumulative, cost] = await Promise.all([
        serviceClient.getStartupSnapshot({
          requestLogLimit: 0, dayStartTs, dayEndTs,
          includeApiModels: false, includeApiKeys: false,
          includeAccountRuntime: false, includeAccountDetails: false,
          includeAccounts: false, includeUsageSnapshots: false,
        }),
        serviceClient.getRequestLogSummary(),
        serviceClient.getCostBreakdown(),
      ]);
      return { snapshot, cumulative, cost };
    },
  });
  const snapshot = overview.data?.snapshot;
  const usage = snapshot?.requestLogTodaySummary;
  // 顶部累计指标不受本地日界限或曲线标签影响，直接使用无时间筛选的服务端汇总。
  const cumulative = overview.data?.cumulative;
  const cost = overview.data?.cost;
  const metrics = [
    { label: "累计 Token", value: formatCompactTokenAmount(cost?.tokens.total), icon: Layers3, detail: "输入 + 输出合计" },
    { label: "累计费用", value: cost ? `$${cost.total.toFixed(4)}` : undefined, icon: Coins, detail: "按本地价格表估算，非实际账单" },
    { label: "累计成功请求", value: cumulative?.successCount, icon: Activity, detail: "成功请求" },
    { label: "可用账号", value: snapshot ? `${snapshot.accountSummary.availableCount} / ${snapshot.accountSummary.accountCount}` : undefined, icon: Users, detail: "当前健康可调用的账号" },
  ];

  return (
    <div className="mx-auto max-w-[1440px] space-y-6 pb-6" data-testid="personal-dashboard">
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className={`size-2 rounded-full ${service.connected ? "bg-emerald-500" : "bg-muted-foreground"}`} />
            {t(service.connected ? "服务已连接" : "正在等待服务连接")}
            <span aria-hidden="true">·</span>
            <span>{new Date(dayStartTs * 1000).toLocaleDateString()}</span>
          </div>
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
            {index === 0 && <dl className="mt-3 space-y-1 text-xs tabular-nums">{([["输入", cost?.tokens.input], ["输出", cost?.tokens.output], ["缓存", cost?.tokens.cache], ["总计", cost?.tokens.total]] as const).map(([name, amount]) => <div key={name} className="flex justify-between gap-2"><dt className="text-muted-foreground">{t(name)}</dt><dd>{formatCompactTokenAmount(amount)}</dd></div>)}</dl>}
            {index === 1 && <dl className="mt-3 space-y-1 text-xs tabular-nums">{([["输入", cost?.input], ["输出", cost?.output], ["缓存", cost?.cache], ["总计", cost?.total]] as const).map(([name, amount]) => <div key={name} className="flex justify-between gap-2"><dt className="text-muted-foreground">{t(name)}</dt><dd>{amount == null ? "—" : `$${amount.toFixed(4)}`}</dd></div>)}</dl>}
            <p className="mt-3 text-xs text-muted-foreground">{index === 2 ? `${t("异常请求")} · ${cumulative?.errorCount ?? "—"}` : t(detail)}</p>
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
      <DashboardRequests />
    </div>
  );
}
