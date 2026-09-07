"use client";

import { AlertTriangle, Check, ArrowRight, PieChart } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import {
  formatCompactTokenAmount,
  formatPercent,
} from "@/lib/dashboard/format";
import { useI18n } from "@/lib/i18n/provider";
import { cn } from "@/lib/utils";
import { buildStaticRouteUrl } from "@/lib/utils/static-routes";

interface DashboardGatewayStatusProps {
  connected: boolean;
  directMode: boolean;
  stats: {
    total: number;
    available: number;
    unavailable: number;
    todayTokens: number;
    cachedTokens: number;
    reasoningTokens: number;
    todayCost: number;
  };
  isLoading: boolean;
}

interface DashboardPoolRemainingProps {
  primary: number | null;
  secondary: number | null;
  primaryKnownCount: number;
  primaryBucketCount: number;
  secondaryKnownCount: number;
  secondaryBucketCount: number;
  isLoading: boolean;
}

function formatUsd(value: number): string {
  return `$${Math.max(0, value || 0).toFixed(2)}`;
}

function StatusMetric({
  label,
  value,
  tone = "text-foreground",
  className,
  valueClassName,
}: {
  label: string;
  value: string;
  tone?: string;
  className?: string;
  valueClassName?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-[68px] flex-col justify-center border-t border-border/55 px-4 first:border-t-0 sm:border-l sm:border-t-0 sm:first:border-l-0 xl:min-h-[76px] xl:px-5",
        className,
      )}
    >
      <span className="truncate text-compact text-muted-foreground">{label}</span>
      <span
        className={cn(
          "mt-0.5 truncate font-mono text-2xl font-semibold leading-none",
          tone,
          valueClassName,
        )}
      >
        {value}
      </span>
    </div>
  );
}

export function DashboardGatewayStatus({
  connected,
  directMode,
  stats,
  isLoading,
}: DashboardGatewayStatusProps) {
  const { t } = useI18n();

  if (isLoading) {
    return <Skeleton className="h-[148px] rounded-xl xl:h-[176px] xl:rounded-2xl" />;
  }

  const title = directMode
    ? t("当前为账号直连模式")
    : connected
      ? t("网关运行正常")
      : t("正在等待网关连接");
  const description = directMode
    ? t("CodexManager 无法统计 CLI 请求日志和用量。")
    : connected
      ? t("近期请求路由稳定，账号池可正常参与调度。")
      : t("正在等待服务连接。");
  const actionHref = directMode ? "/platform-mode" : "/logs";
  const actionLabel = directMode ? t("去切换为本地网关") : t("查看异常请求");

  return (
    <Card className="dashboard-primary-panel routing-command-card glass-card overflow-hidden rounded-xl border-border/60 py-0 xl:rounded-2xl">
      <CardContent className="p-0">
        <div className="flex min-h-[80px] flex-col gap-3 px-4 py-3 lg:flex-row lg:items-center lg:justify-between xl:min-h-[92px] xl:gap-4 xl:px-5 xl:py-4">
          <div className="flex min-w-0 items-center gap-3 xl:gap-4">
            <div
              className={cn(
                "flex h-9 w-9 shrink-0 items-center justify-center rounded-full border-2 bg-background/75 shadow-[0_8px_24px_-18px_currentColor] xl:h-10 xl:w-10",
                connected && !directMode
                  ? "border-emerald-500 text-emerald-600"
                  : "border-amber-500/45 text-amber-600",
              )}
            >
              {connected && !directMode ? (
                <Check className="h-[18px] w-[18px] stroke-[2.5] xl:h-5 xl:w-5" />
              ) : (
                <AlertTriangle className="h-[18px] w-[18px] xl:h-5 xl:w-5" />
              )}
            </div>
            <div className="min-w-0">
              <h2 className="text-xl font-semibold leading-tight tracking-[-0.02em] text-foreground">
                {title}
              </h2>
              <p className="mt-1 max-w-2xl text-sm leading-5 text-muted-foreground xl:mt-2 xl:leading-6">
                {description}
              </p>
            </div>
          </div>
          <a
            href={buildStaticRouteUrl(actionHref)}
            className={cn(
              buttonVariants({ size: "lg" }),
              "command-center-primary-action h-9 min-w-[124px] shrink-0 rounded-lg px-4 text-sm xl:h-10 xl:min-w-[136px] xl:px-5",
            )}
          >
            {actionLabel}
            <ArrowRight className="ml-1 h-3.5 w-3.5" />
          </a>
        </div>

        <div className="grid border-t border-border/55 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-7">
          <StatusMetric
            label={t("服务连接")}
            value={connected ? t("正常") : t("离线")}
            tone={connected ? "text-emerald-600" : "text-rose-600"}
          />
          <StatusMetric label={t("账号")} value={String(stats.total)} />
          <StatusMetric label={t("可用")} value={String(stats.available)} tone="text-emerald-600" />
          <StatusMetric
            label={t("异常")}
            value={String(stats.unavailable)}
            tone={stats.unavailable > 0 ? "text-rose-600" : "text-foreground"}
          />
          <StatusMetric
            className="lg:col-span-2"
            label={t("今日/缓存/推理 用量")}
            value={`${formatCompactTokenAmount(stats.todayTokens)} / ${formatCompactTokenAmount(stats.cachedTokens)} / ${formatCompactTokenAmount(stats.reasoningTokens)}`}
            valueClassName="text-dense tracking-[-0.035em]"
          />
          <StatusMetric
            label={t("预计费用")}
            value={formatUsd(stats.todayCost)}
            valueClassName="text-metric-emphasis"
          />
        </div>
      </CardContent>
    </Card>
  );
}

function PoolBucket({
  label,
  value,
  knownCount,
  bucketCount,
  tone,
}: {
  label: string;
  value: number | null;
  knownCount: number;
  bucketCount: number;
  tone: "emerald" | "blue";
}) {
  const normalizedValue = value == null ? 0 : Math.max(0, Math.min(100, value));
  const isEmerald = tone === "emerald";

  return (
    <div className="min-w-0">
      <div className="mb-1.5 flex items-center justify-between gap-3 text-xs xl:mb-2 xl:text-sm">
        <span className="font-medium text-muted-foreground">{label}</span>
        <span
          className={cn(
            "font-mono font-semibold",
            isEmerald ? "text-emerald-600" : "text-blue-600",
          )}
        >
          {formatPercent(value)}
        </span>
      </div>
      <Progress
        value={normalizedValue}
        className="gap-0"
        trackClassName={cn(
          "h-1.5 xl:h-2",
          isEmerald ? "bg-emerald-500/18" : "bg-blue-500/18",
        )}
        indicatorClassName={isEmerald ? "bg-emerald-500" : "bg-blue-500"}
      />
      <div className="mt-1.5 truncate font-mono text-[10px] text-muted-foreground xl:mt-2 xl:text-xs">
        {knownCount}/{bucketCount}
      </div>
    </div>
  );
}

export function DashboardPoolRemaining({
  primary,
  secondary,
  primaryKnownCount,
  primaryBucketCount,
  secondaryKnownCount,
  secondaryBucketCount,
  isLoading,
}: DashboardPoolRemainingProps) {
  const { t } = useI18n();

  if (isLoading) {
    return <Skeleton className="h-[84px] rounded-xl xl:rounded-2xl" />;
  }

  return (
    <Card className="dashboard-pool-remaining dashboard-primary-panel glass-card overflow-hidden rounded-xl border-border/60 py-0 xl:rounded-2xl">
      <CardContent className="grid gap-4 px-4 py-4 md:grid-cols-[200px_minmax(0,1fr)] md:items-center xl:grid-cols-[210px_minmax(0,1fr)_minmax(0,1fr)]">
        <div className="flex min-w-0 items-center gap-3">
          <PieChart className="h-5 w-5 shrink-0 text-emerald-600 xl:h-6 xl:w-6" />
          <span className="truncate text-sm font-semibold text-foreground xl:text-lg">
            {t("账号池剩余")}
          </span>
          <Badge
            variant="outline"
            className="h-6 shrink-0 rounded-md border-emerald-500/25 bg-emerald-500/8 px-2 text-[10px] font-semibold text-emerald-700 xl:h-7 xl:text-xs"
          >
            POOL
          </Badge>
        </div>
        <div className="grid min-w-0 gap-4 sm:grid-cols-2 md:col-span-1 xl:col-span-2">
          <PoolBucket
            label={t("5小时内")}
            value={primary}
            knownCount={primaryKnownCount}
            bucketCount={primaryBucketCount}
            tone="emerald"
          />
          <PoolBucket
            label={t("7天内")}
            value={secondary}
            knownCount={secondaryKnownCount}
            bucketCount={secondaryBucketCount}
            tone="blue"
          />
        </div>
      </CardContent>
    </Card>
  );
}
