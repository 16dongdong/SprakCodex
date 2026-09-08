"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Network, Route } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { observationClient, observationQueryKey } from "@/lib/api/observationClient";
import {
  sessionRoutingClient,
  sessionRoutingQueryKey,
  type SessionRoutingStatus,
} from "@/lib/api/sessionRoutingClient";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";
import type { Account } from "@/types";

interface SessionRoutingPanelProps {
  accounts: Account[];
}

// 账号页仅保留接入和分流总开关；普通容器不扩大点击范围，按钮使用独立无障碍名称。
export function SessionRoutingPanel({ accounts }: SessionRoutingPanelProps) {
  const { t } = useI18n();
  const service = useAppStore((state) => state.serviceStatus);
  const queryClient = useQueryClient();
  const routingKey = [...sessionRoutingQueryKey, service.addr];
  const observationKey = [...observationQueryKey, service.addr];
  const routingQuery = useQuery({
    queryKey: routingKey,
    queryFn: sessionRoutingClient.status,
    enabled: service.connected,
    refetchInterval: 5000,
  });
  const observationQuery = useQuery({
    queryKey: observationKey,
    queryFn: observationClient.status,
    enabled: service.connected,
    refetchInterval: 5000,
  });
  const observationMutation = useMutation({
    mutationFn: (running: boolean) =>
      running ? observationClient.start() : observationClient.stop(),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(observationKey, snapshot);
    },
  });
  const updateSnapshot = (snapshot: SessionRoutingStatus) => {
    queryClient.setQueryData(routingKey, snapshot);
  };
  const enabledMutation = useMutation({
    mutationFn: sessionRoutingClient.setEnabled,
    onSuccess: updateSnapshot,
  });
  const routing = routingQuery.data;
  const preferenceByAccount = new Map(
    (routing?.accounts ?? []).map((item) => [item.accountId, item]),
  );
  const availableCount = accounts.filter(
    (account) =>
      account.isAvailable && (preferenceByAccount.get(account.id)?.enabled ?? true),
  ).length;
  const error =
    routingQuery.error ||
    observationQuery.error ||
    observationMutation.error ||
    enabledMutation.error;

  return (
    <section
      className="glass-card mb-4 space-y-4 rounded-2xl border border-border/70 p-4"
      data-testid="session-routing-panel"
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
          <div
            className="inline-flex items-center gap-2"
          >
            <Network className="size-4" />
            {t("本机接入")}
            {observationMutation.isPending && (
              <Loader2 className="size-3 animate-spin" />
            )}
            <Switch
              id="local-access-enabled"
              aria-label={t("本机接入")}
              checked={observationQuery.data?.running ?? false}
              disabled={
                !service.connected ||
                observationQuery.isLoading ||
                observationMutation.isPending
              }
              onCheckedChange={(running) => observationMutation.mutate(running)}
            />
          </div>
          <span>
            {t("可用账号：{available}/{total}", {
              available: availableCount,
              total: accounts.length,
            })}
          </span>
          <span>
            {t("活跃绑定：{count}", { count: routing?.activeBindingCount ?? 0 })}
          </span>
        </div>
        <div
          className="flex items-center gap-3 text-sm font-medium"
        >
          <Route className="size-4" />
          {t("会话账号分流")}
          {(enabledMutation.isPending || routingQuery.isLoading) && (
            <Loader2 className="size-3 animate-spin" />
          )}
          <Switch
            id="session-routing-enabled"
            aria-label={t("会话账号分流")}
            checked={routing?.enabled ?? false}
            disabled={
              !service.connected ||
              enabledMutation.isPending ||
              routingQuery.isLoading
            }
            onCheckedChange={(enabled) => enabledMutation.mutate(enabled)}
          />
        </div>
      </div>

      <p className="text-xs text-muted-foreground">
        {t(
          "关闭时保留 Codex 原始凭据并记录；开启后仅为新会话均衡分配账号，同一会话保持固定绑定。",
        )}
      </p>

      {error && (
        <p className="text-xs text-destructive">
          {error instanceof Error ? error.message : t("读取会话分流状态失败")}
        </p>
      )}
    </section>
  );
}
