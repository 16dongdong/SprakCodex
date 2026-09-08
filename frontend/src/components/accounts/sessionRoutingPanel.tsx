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

/// 在账号页集中展示本机接入、总开关和账号参与状态；所有变更以后端返回快照为准。
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
  const accountMutation = useMutation({
    mutationFn: ({ accountId, enabled }: { accountId: string; enabled: boolean }) =>
      sessionRoutingClient.setAccountEnabled(accountId, enabled),
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
    enabledMutation.error ||
    accountMutation.error;

  return (
    <section
      className="glass-card mb-4 space-y-4 rounded-2xl border border-border/70 p-4"
      data-testid="session-routing-panel"
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
          <label
            htmlFor="local-access-enabled"
            className="inline-flex items-center gap-2"
          >
            <Network className="size-4" />
            {t("本机接入")}
            {observationMutation.isPending && (
              <Loader2 className="size-3 animate-spin" />
            )}
            <Switch
              id="local-access-enabled"
              checked={observationQuery.data?.running ?? false}
              disabled={
                !service.connected ||
                observationQuery.isLoading ||
                observationMutation.isPending
              }
              onCheckedChange={(running) => observationMutation.mutate(running)}
            />
          </label>
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
        <label
          htmlFor="session-routing-enabled"
          className="flex items-center gap-3 text-sm font-medium"
        >
          <Route className="size-4" />
          {t("会话账号分流")}
          {(enabledMutation.isPending || routingQuery.isLoading) && (
            <Loader2 className="size-3 animate-spin" />
          )}
          <Switch
            id="session-routing-enabled"
            checked={routing?.enabled ?? false}
            disabled={
              !service.connected ||
              enabledMutation.isPending ||
              routingQuery.isLoading
            }
            onCheckedChange={(enabled) => enabledMutation.mutate(enabled)}
          />
        </label>
      </div>

      <p className="text-xs text-muted-foreground">
        {t(
          "关闭时保留 Codex 原始凭据并记录；开启后仅为新会话均衡分配账号，同一会话保持固定绑定。",
        )}
      </p>

      {routing?.enabled && accounts.length > 0 && (
        <div className="grid gap-2 border-t border-border/60 pt-3 sm:grid-cols-2 xl:grid-cols-3">
          {accounts.map((account) => {
            const preference = preferenceByAccount.get(account.id);
            const enabled = preference?.enabled ?? true;
            return (
              <label
                key={account.id}
                className="flex min-w-0 items-center justify-between gap-3 rounded-lg border border-border/60 bg-background/35 px-3 py-2 text-sm"
              >
                <span className="min-w-0 truncate" title={account.label}>
                  {account.label}
                  <span className="ml-2 text-xs text-muted-foreground">
                    {t("绑定 {count}", {
                      count: preference?.activeBindingCount ?? 0,
                    })}
                  </span>
                </span>
                <Switch
                  checked={enabled}
                  disabled={accountMutation.isPending}
                  aria-label={`${account.label}${t("参与分流")}`}
                  onCheckedChange={(nextEnabled) =>
                    accountMutation.mutate({
                      accountId: account.id,
                      enabled: nextEnabled,
                    })
                  }
                />
              </label>
            );
          })}
        </div>
      )}

      {error && (
        <p className="text-xs text-destructive">
          {error instanceof Error ? error.message : t("读取会话分流状态失败")}
        </p>
      )}
    </section>
  );
}
