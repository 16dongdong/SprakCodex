"use client";

import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { formatCompactTokenAmount } from "@/lib/dashboard/format";
import { ArrowUpRight, Inbox } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import type { RequestLogListWithSummaryResult } from "@/types";

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

// 最近请求按时间升序绘制单次 Token，而非累计日用量；未知值保留断点，隐藏缓存页不挂载尺寸观察器。
export function DashboardRequests({ requests, loading }: { requests?: RequestLogListWithSummaryResult; loading: boolean }) {
  const { t } = useI18n();
  const active = useDesktopPageActive("/");
  const navigate = useAppStore((state) => state.navigateShellPath);
  const points = [...(requests?.items ?? [])]
    .filter((request) => request.createdAt != null)
    .sort((left, right) => left.createdAt! - right.createdAt!)
    .map((request) => ({ time: request.createdAt! * 1000, tokens: request.totalTokens }));
  return (
    <section className="glass-card overflow-hidden rounded-2xl border border-border/60">
      <header className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 sm:px-6">
        <div><h2 className="font-semibold">{t("请求 Token 曲线")}</h2><p className="mt-1 text-xs text-muted-foreground">{t("最近 100 条请求 · 单次 Token 用量")}</p></div>
        <Button variant="ghost" size="sm" onClick={() => navigate("/logs")}>{t("请求日志")}<ArrowUpRight className="ml-1 size-4" /></Button>
      </header>
      <div className="h-72 min-w-0 px-3 pb-5 sm:px-5" aria-label={t("请求 Token 曲线")}>
        {active && points.length > 0 ? (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={points} margin={{ top: 16, right: 16, bottom: 4, left: 0 }} accessibilityLayer>
              <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 5" />
              <XAxis dataKey="time" tickFormatter={(value) => new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })} minTickGap={50} tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} />
              <YAxis tickFormatter={formatCompactTokenAmount} width={68} tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} />
              <Tooltip labelFormatter={(value) => new Date(Number(value)).toLocaleTimeString()} formatter={(value) => [formatCompactTokenAmount(typeof value === "number" ? value : null), "Token"]} contentStyle={{ background: "var(--card)", borderColor: "var(--border)", borderRadius: 12, color: "var(--foreground)" }} />
              <Area type="monotone" dataKey="tokens" name="Token" stroke="var(--primary)" fill="var(--primary)" fillOpacity={0.08} strokeWidth={2.5} dot={points.length === 1} activeDot={{ r: 4 }} connectNulls={false} isAnimationActive={false} />
            </AreaChart>
          </ResponsiveContainer>
        ) : <div role="status" className="flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground"><Inbox className="size-8 opacity-50" />{loading ? t("正在恢复页面内容，请稍候...") : requests ? t("暂无请求日志") : "—"}</div>}
      </div>
    </section>
  );
}
