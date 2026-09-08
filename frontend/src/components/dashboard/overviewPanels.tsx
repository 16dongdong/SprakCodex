"use client";

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

// 固定长度的最近请求只展示单次指标；空库、加载和未知状态各自呈现，详细查询交给日志页。
export function DashboardRequests({ requests, loading }: { requests?: RequestLogListWithSummaryResult; loading: boolean }) {
  const { t } = useI18n();
  const navigate = useAppStore((state) => state.navigateShellPath);
  return (
    <section className="glass-card overflow-hidden rounded-2xl border border-border/60">
      <header className="flex items-center justify-between gap-3 px-5 py-4 sm:px-6">
        <h2 className="font-semibold">{t("最近日志")}<span className="ml-3 rounded-md bg-muted px-2 py-1 text-xs font-normal tabular-nums text-muted-foreground">{requests?.total.toLocaleString() ?? "—"}</span></h2>
        <Button variant="ghost" size="sm" onClick={() => navigate("/logs")}>{t("请求日志")}<ArrowUpRight className="ml-1 size-4" /></Button>
      </header>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[580px] text-left text-sm">
          <thead className="border-y border-border/50 bg-muted/30 text-xs text-muted-foreground"><tr>
            {["时间", "账号", "模型", "状态", "总使用 Token"].map((label) => <th key={label} scope="col" className="px-6 py-3 font-medium">{t(label)}</th>)}
          </tr></thead>
          <tbody>{requests?.items.map((request) => (
            <tr key={request.id} className="border-b border-border/30 last:border-0 hover:bg-muted/25">
              <td className="whitespace-nowrap px-6 py-3.5 text-xs tabular-nums text-muted-foreground">{request.createdAt ? new Date(request.createdAt * 1000).toLocaleTimeString() : "—"}</td>
              <td className="max-w-48 truncate px-6 py-3.5" title={request.accountLabel || request.accountId}>{request.accountLabel || request.accountId || "—"}</td>
              <td className="max-w-56 truncate px-6 py-3.5 font-medium" title={request.model}>{request.model || "—"}</td>
              <td className="px-6 py-3.5"><span className={`rounded-md px-2 py-1 text-xs tabular-nums ${request.statusCode == null ? "bg-muted text-muted-foreground" : request.statusCode >= 400 ? "bg-destructive/10 text-destructive" : "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"}`}>{request.statusCode ?? "—"}</span></td>
              <td className="px-6 py-3.5 tabular-nums">{request.totalTokens?.toLocaleString() ?? "—"}</td>
            </tr>
          ))}</tbody>
        </table>
      </div>
      {!requests?.items.length && <div role="status" className="flex flex-col items-center gap-3 py-12 text-sm text-muted-foreground"><Inbox className="size-8 opacity-50" />{loading ? t("正在恢复页面内容，请稍候...") : requests ? t("暂无请求日志") : "—"}</div>}
    </section>
  );
}
