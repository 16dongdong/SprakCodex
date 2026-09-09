"use client";

import { Zap } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/lib/i18n/provider";
import {
  formatModelEffortDisplay,
  normalizeRequestType,
  RequestTypeBadge,
  resolveAccountDisplayNameById,
  resolveDisplayRequestPath,
  resolveDisplayServiceTier,
  resolveFriendlyRequestPathLabel,
  resolveUpstreamDisplay,
  ServiceTierBadge,
} from "./page-helpers";
import type { RequestLog, RequestLogFilterSummary } from "@/types";

const logTooltipContentClassName = "logs-tooltip-content";
const logTooltipLabelClassName = "text-[10px] font-medium text-muted-foreground";

export function AccountKeyInfoCell({
  log,
  accountLabel,
  accountNameMap,
}: {
  log: RequestLog;
  accountLabel: string;
  accountNameMap: Map<string, string>;
}) {
  const { t } = useI18n();
  const displayAccount = accountLabel || log.accountId || "-";
  const sessionId = log.actualSourceKind === "session" ? log.actualSourceId : "";
  const routingMode = log.routeStrategy || "passthrough";

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex max-w-[190px] flex-col gap-1">
          <div className="flex items-center gap-1">
            <Zap className="h-3 w-3 text-yellow-500" />
            <span className="truncate text-[11px] font-medium">{displayAccount}</span>
          </div>
          <span className="truncate font-mono text-[10px] text-muted-foreground">
            {routingMode}
          </span>
        </div>
      </TooltipTrigger>
      <TooltipContent className={`${logTooltipContentClassName} max-w-sm`}>
        <div className="flex min-w-[240px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("邮箱 / 名称")}</div>
            <div className="break-all text-[11px]">{displayAccount}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("账号 ID")}</div>
            <div className="break-all font-mono text-[11px]">{log.accountId || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("分流方式")}</div>
            <div className="break-all font-mono text-[11px]">{routingMode}</div>
          </div>
          {sessionId ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("会话标识")}</div>
              <div className="break-all font-mono text-[11px]">{sessionId}</div>
            </div>
          ) : null}
          {log.accountId ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("账号名称")}</div>
              <div className="break-all text-[11px]">
                {resolveAccountDisplayNameById(log.accountId, accountNameMap)}
              </div>
            </div>
          ) : null}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

// 仅展示记录实际具备的路由字段；客户端完成事件没有网络来源时显示未知，不补造协议来源或地址。
export function RequestRouteInfoCell({ log }: { log: RequestLog }) {
  const { t } = useI18n();
  const displayPath = resolveDisplayRequestPath(log) || "-";
  const displayPathLabel = resolveFriendlyRequestPathLabel(displayPath, t) || "-";
  const recordedPath = String(log.path || log.requestPath || "").trim();
  const originalPath = String(log.originalPath || "").trim();
  const adaptedPath = String(log.adaptedPath || "").trim();
  const gatewayMode = String(log.gatewayMode || "").trim().toLowerCase();
  const isCompactGatewayMode = gatewayMode === "compact";
  const upstreamUrl = String(log.upstreamUrl || "").trim();
  const upstreamDisplay = resolveUpstreamDisplay(upstreamUrl, t);
  const forwardedPath = adaptedPath && adaptedPath !== displayPath ? adaptedPath : "";
  const friendlyDisplayPath =
    isCompactGatewayMode
      ? t("上下文压缩")
      : displayPathLabel && displayPathLabel !== displayPath
        ? displayPathLabel
        : "";
  const requestType = normalizeRequestType(log.requestType);
  const canonicalSource = String(log.canonicalSource || (requestType === "client" ? "-" : "native_codex")).trim();
  const sizeRejectStage = String(log.sizeRejectStage || "-").trim();
  const routeStrategy = String(log.routeStrategy || "").trim();
  const routeSource = String(log.routeSource || "").trim();

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex flex-col gap-0.5">
          <div className="flex items-center gap-1.5">
            <RequestTypeBadge requestType={requestType} />
            {isCompactGatewayMode ? (
              <Badge className="h-5 rounded-full border-amber-500/20 bg-amber-500/10 px-1.5 text-[10px] font-medium text-amber-500">
                {t("压缩")}
              </Badge>
            ) : null}
            <span className="shrink-0 whitespace-nowrap font-bold text-primary">{log.method || "-"}</span>
          </div>
          <span className="max-w-[220px] truncate font-mono text-[11px] text-foreground">
            {displayPath}
          </span>
          {friendlyDisplayPath ? (
            <span className="max-w-[220px] truncate text-[10px] text-muted-foreground">
              {friendlyDisplayPath}
            </span>
          ) : null}
          {forwardedPath ? (
            <span className="max-w-[220px] truncate font-mono text-[10px] text-amber-500">
              -&gt; {forwardedPath}
            </span>
          ) : null}
          {upstreamDisplay ? (
            <span className="max-w-[220px] truncate font-mono text-[10px] text-cyan-500">
              =&gt; {upstreamDisplay}
            </span>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent className={`${logTooltipContentClassName} max-w-md`}>
        <div className="flex min-w-[280px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("请求类型")}</div>
            <div className="font-mono text-[11px] uppercase">{requestType === "client" ? t("客户端事件") : requestType}</div>
          </div>
          {gatewayMode ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("网关模式")}</div>
              <div className="font-mono text-[11px]">{gatewayMode}</div>
            </div>
          ) : null}
          {routeStrategy ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("路由策略")}</div>
              <div className="font-mono text-[11px]">{routeStrategy}</div>
            </div>
          ) : null}
          {routeSource ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("路由来源")}</div>
              <div className="font-mono text-[11px]">{routeSource}</div>
            </div>
          ) : null}
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("规范来源")}</div>
            <div className="font-mono text-[11px]">{canonicalSource}</div>
          </div>
          {sizeRejectStage && sizeRejectStage !== "-" ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("大小拒绝阶段")}</div>
              <div className="font-mono text-[11px]">{sizeRejectStage}</div>
            </div>
          ) : null}
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("方法")}</div>
            <div className="font-mono text-[11px]">{log.method || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("显示名称")}</div>
            <div className="break-all text-[11px]">{displayPathLabel}</div>
          </div>
          {displayPath && displayPathLabel !== displayPath ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("原始路径")}</div>
              <div className="break-all font-mono text-[11px]">{displayPath}</div>
            </div>
          ) : null}
          {recordedPath && recordedPath !== displayPath ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("记录地址")}</div>
              <div className="break-all font-mono text-[11px]">{recordedPath}</div>
            </div>
          ) : null}
          {originalPath && originalPath !== displayPath ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("原始地址")}</div>
              <div className="break-all font-mono text-[11px]">{originalPath}</div>
            </div>
          ) : null}
          {forwardedPath ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("转发路径")}</div>
              <div className="break-all font-mono text-[11px]">{forwardedPath}</div>
            </div>
          ) : null}
          {log.responseAdapter ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("适配器")}</div>
              <div className="break-all font-mono text-[11px]">{log.responseAdapter}</div>
            </div>
          ) : null}
          {upstreamDisplay ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("上游")}</div>
              <div className="break-all font-mono text-[11px]">{upstreamDisplay}</div>
            </div>
          ) : null}
          {upstreamUrl ? (
            <div className="space-y-0.5">
              <div className={logTooltipLabelClassName}>{t("上游地址")}</div>
              <div className="break-all font-mono text-[11px]">{upstreamUrl}</div>
            </div>
          ) : null}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function ErrorInfoCell({ error }: { error: string }) {
  const text = String(error || "").trim();
  if (!text) {
    return <span className="text-muted-foreground">-</span>;
  }

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <span className="block max-w-[220px] truncate font-medium text-red-400">
          {text}
        </span>
      </TooltipTrigger>
      <TooltipContent className={`${logTooltipContentClassName} max-w-md`}>
        <div className="max-w-[360px] break-all font-mono text-[11px]">{text}</div>
      </TooltipContent>
    </Tooltip>
  );
}

export function ModelEffortCell({ log }: { log: RequestLog }) {
  const { t } = useI18n();
  const clientModel = String(log.clientModel || "").trim() || "-";
  const model = String(log.model || "").trim();
  const modelSource = String(log.modelSource || "").trim() || "-";
  const upstreamModel = String(log.upstreamModel || "").trim();
  const actualSourceKind = String(log.actualSourceKind || "").trim();
  const actualSourceId = String(log.actualSourceId || "").trim();
  const clientReasoningEffort = String(log.clientReasoningEffort || "").trim() || "-";
  const effort = String(log.reasoningEffort || "").trim();
  const reasoningSource = String(log.reasoningSource || "").trim() || "-";
  const clientServiceTier = resolveDisplayServiceTier(log.serviceTier);
  const effectiveServiceTier = resolveDisplayServiceTier(
    log.effectiveServiceTier || log.serviceTier,
  );
  const serviceTierSource = String(log.serviceTierSource || "").trim() || "-";
  const badgeServiceTier =
    effectiveServiceTier !== "auto" ? effectiveServiceTier : clientServiceTier;
  const display = formatModelEffortDisplay(log);
  const forwardedModel = upstreamModel && upstreamModel !== model ? upstreamModel : "";

  return (
    <Tooltip>
      <TooltipTrigger render={<div />} className="block text-left">
        <div className="flex flex-col gap-1">
          <span className="block max-w-[200px] truncate font-medium text-foreground">
            {display}
          </span>
          {forwardedModel ? (
            <span className="block max-w-[200px] truncate font-mono text-[10px] text-amber-500">
              {t("转发")} {forwardedModel}
            </span>
          ) : null}
          <ServiceTierBadge serviceTier={badgeServiceTier} />
        </div>
      </TooltipTrigger>
      <TooltipContent className={`${logTooltipContentClassName} max-w-sm`}>
        <div className="flex min-w-[220px] flex-col gap-2">
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("客户端模型")}</div>
            <div className="break-all font-mono text-[11px]">{clientModel}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("最终平台模型")}</div>
            <div className="break-all font-mono text-[11px]">{model || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("模型来源")}</div>
            <div className="break-all font-mono text-[11px]">{modelSource}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("上游模型")}</div>
            <div className="break-all font-mono text-[11px]">{upstreamModel || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("实际来源")}</div>
            <div className="break-all font-mono text-[11px]">
              {actualSourceKind && actualSourceId
                ? `${actualSourceKind}:${actualSourceId}`
                : actualSourceKind || actualSourceId || "-"}
            </div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("客户端推理")}</div>
            <div className="break-all font-mono text-[11px]">{clientReasoningEffort}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("最终推理")}</div>
            <div className="break-all font-mono text-[11px]">{effort || "-"}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("推理来源")}</div>
            <div className="break-all font-mono text-[11px]">{reasoningSource}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("客户端显式服务等级")}</div>
            <div className="break-all font-mono text-[11px]">{clientServiceTier}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("最终生效服务等级")}</div>
            <div className="break-all font-mono text-[11px]">{effectiveServiceTier}</div>
          </div>
          <div className="space-y-0.5">
            <div className={logTooltipLabelClassName}>{t("服务等级来源")}</div>
            <div className="break-all font-mono text-[11px]">{serviceTierSource}</div>
          </div>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function buildSummaryPlaceholder(
  logs: RequestLog[],
): RequestLogFilterSummary {
  const successCount = logs.filter((item) => {
    const statusCode = item.statusCode ?? 0;
    return statusCode >= 200 && statusCode < 300 && !String(item.error || "").trim();
  }).length;
  const errorCount = logs.filter((item) => {
    const statusCode = item.statusCode;
    return Boolean(String(item.error || "").trim()) || (statusCode != null && statusCode >= 400);
  }).length;
  const totalTokens = logs.reduce((sum, item) => sum + Math.max(0, item.totalTokens || 0), 0);
  const totalCostUsd = logs.reduce(
    (sum, item) => sum + Math.max(0, item.estimatedCostUsd || 0),
    0,
  );

  return {
    totalCount: logs.length,
    filteredCount: logs.length,
    successCount,
    errorCount,
    totalTokens,
    totalCostUsd,
  };
}
