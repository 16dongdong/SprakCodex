"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useLocalDayRange } from "@/hooks/useLocalDayRange";
import { serviceClient } from "@/lib/api/service-client";
import { getChartBoundaries, type ChartPeriod } from "@/lib/dashboard/chartRange";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { formatCompactTokenAmount } from "@/lib/dashboard/format";
import { ArrowUpRight, Inbox } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";

// 未知额度保留破折号；仅把进度条视觉范围限制在 0–100，不把缺失状态伪装为满额。
export function DashboardQuota({ label, value }: { label: string; value?: number | null }) {
  const known = value != null && Number.isFinite(value);
  const percent = known ? Math.max(0, Math.min(100, value)) : 0;
  return (
    <div>
      <div className="mb-2 flex justify-between text-xs"><span className="text-muted-foreground">{label}</span><span className="font-medium tabular-nums">{known ? `${value.toFixed(0)}%` : "—"}</span></div>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted" role={known ? "meter" : undefined} aria-label={label} aria-valuemin={known ? 0 : undefined} aria-valuemax={known ? 100 : undefined} aria-valuenow={known ? percent : undefined}>
        <div className={`h-full rounded-full ${percent < 20 ? "bg-amber-500" : "bg-primary/70"}`} style={{ width: `${percent}%` }} />
      </div>
    </div>
  );
}

// 按完整周期汇总所有请求；已过去的空桶显示零，未来桶保留空白，隐藏页不挂载图表。
export function DashboardRequests() {
  const { t } = useI18n();
  const active = useDesktopPageActive("/");
  const navigate = useAppStore((state) => state.navigateShellPath);
  const [period, setPeriod] = useState<ChartPeriod>("day");
  const service = useAppStore((state) => state.serviceStatus);
  const { dayStartTs } = useLocalDayRange();
  const boundaries = getChartBoundaries(period, dayStartTs);
  const history = useQuery({
    queryKey: ["dashboardChart", service.addr, period, boundaries],
    enabled: active && service.connected,
    refetchInterval: active && service.connected ? 15_000 : false,
    queryFn: () => serviceClient.getTokenSeries(boundaries),
  });
  const pending = history.isPending && service.connected;
  const points = (history.data ?? []).map((point) => ({ ...point, tokens: point.time > history.dataUpdatedAt ? null : point.tokens }));
  return (
    <section className="glass-card overflow-hidden rounded-2xl border border-border/60">
      <header className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 sm:px-6">
        <div><h2 className="font-semibold">{t("请求 Token 曲线")}</h2><p className="mt-1 text-xs text-muted-foreground">{t(period === "day" ? "本日按小时汇总全部请求" : "本周期按天汇总全部请求")}</p></div>
        <div className="flex flex-wrap items-center gap-2"><Tabs value={period} onValueChange={(value) => { if (value === "month" || value === "week" || value === "day") setPeriod(value); }}><TabsList aria-label={t("请求 Token 曲线")}><TabsTrigger value="month">{t("本月")}</TabsTrigger><TabsTrigger value="week">{t("本周")}</TabsTrigger><TabsTrigger value="day">{t("本日")}</TabsTrigger></TabsList></Tabs><Button variant="ghost" size="sm" onClick={() => navigate("/logs")}>{t("请求日志")}<ArrowUpRight className="ml-1 size-4" /></Button></div>
      </header>
      {history.error && <p role="alert" className="px-5 text-sm text-destructive">{history.error.message}</p>}
      <div className="h-72 min-w-0 px-3 pb-5 sm:px-5" aria-label={t("请求 Token 曲线")}>
        {active && points.length > 0 ? (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={points} margin={{ top: 16, right: 16, bottom: 4, left: 0 }} accessibilityLayer>
              <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 5" />
              <XAxis dataKey="time" tickFormatter={(value) => period === "day" ? new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : new Date(value).toLocaleDateString([], { month: "2-digit", day: "2-digit" })} minTickGap={50} tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} />
              <YAxis tickFormatter={formatCompactTokenAmount} width={68} tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} />
              <Tooltip labelFormatter={(value) => new Date(Number(value)).toLocaleString()} formatter={(value) => [formatCompactTokenAmount(typeof value === "number" ? value : null), "Token"]} contentStyle={{ background: "var(--card)", borderColor: "var(--border)", borderRadius: 12, color: "var(--foreground)" }} />
              <Area type="monotone" dataKey="tokens" name="Token" stroke="var(--primary)" fill="var(--primary)" fillOpacity={0.08} strokeWidth={2.5} dot={points.length === 1} activeDot={{ r: 4 }} connectNulls={false} isAnimationActive={false} />
            </AreaChart>
          </ResponsiveContainer>
        ) : <div role="status" className="flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground"><Inbox className="size-8 opacity-50" />{pending ? t("正在恢复页面内容，请稍候...") : history.data ? t("暂无请求日志") : "—"}</div>}
      </div>
    </section>
  );
}
