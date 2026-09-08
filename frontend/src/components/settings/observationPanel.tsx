"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Radio, Loader2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { observationClient, observationQueryKey } from "@/lib/api/observationClient";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";

// 紧凑设置行仅展示开关和计数；服务未连接或请求中禁用切换，失败在当前行明确呈现。
export function ObservationPanel() {
  const { t } = useI18n();
  const service = useAppStore((state) => state.serviceStatus);
  const queryClient = useQueryClient();
  const queryKey = [...observationQueryKey, service.addr];
  const status = useQuery({ queryKey, queryFn: observationClient.status, enabled: service.connected, refetchInterval: 3000 });
  const change = useMutation({
    mutationFn: (running: boolean) => running ? observationClient.stop() : observationClient.start(),
    onSuccess: (result) => { queryClient.setQueryData(queryKey, result); },
  });
  const current = status.data;
  const error = change.error || status.error;

  return (
    <div className="rounded-xl border border-border/70 bg-background/45 p-4" data-testid="observation-panel">
      <div className="flex items-center justify-between gap-3">
        <div className="grid min-w-0 gap-1">
          <label htmlFor="observation-enabled" className="flex items-center gap-2 text-sm font-medium">
            <Radio className="size-4" />{t("直连观测")}
            {change.isPending && <Loader2 className="size-3 animate-spin" />}
          </label>
          <p className="text-xs text-muted-foreground">{t("保留原登录与上游，记录请求和用量；重启自动恢复。")}</p>
          {current?.running && <p className="text-xs text-muted-foreground">{t("状态：运行中 · 已写入 {written} 条 · 写入异常 {errors} 次", { written: current.writtenRequests, errors: current.storageErrors })}</p>}
        </div>
        <Switch id="observation-enabled" aria-label={t("直连观测")} checked={Boolean(current?.running)} disabled={!service.connected || status.isPending || change.isPending} onCheckedChange={() => change.mutate(Boolean(current?.running))} />
      </div>
      {error && <p role="alert" className="mt-2 break-words text-xs text-destructive">{error instanceof Error ? error.message : t("观测操作失败")}</p>}
    </div>
  );
}
