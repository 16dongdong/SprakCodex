"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Radio, Loader2 } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { observationClient, observationQueryKey } from "@/lib/api/observationClient";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";

// 展示运行状态与独立开关，并说明退出和停用的区别；既有客户端的离线完成事件由恢复后的消费者确认。
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
    <Card className="glass-card min-w-0" data-testid="observation-panel">
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-3">
        <CardTitle className="flex items-center gap-2 text-base"><Radio className="size-4" />{t("直连观测")}</CardTitle>
        <Button disabled={!service.connected || status.isPending || change.isPending} onClick={() => change.mutate(Boolean(current?.running))}>
          {change.isPending && <Loader2 className="mr-2 size-4 animate-spin" />}
          {current?.running ? t("关闭观测") : t("启用观测")}
        </Button>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <p className="text-muted-foreground">{t("保留 Codex 原有登录与上游，记录请求、实际 Token 用量和模型价格费用快照，不扣除平台钱包或密钥额度。")}</p>
        <p className="text-muted-foreground">{t("Windows 自动接入已有和新启动的 Codex；已接入客户端的完成事件可在宿主恢复后补录。")}</p>
        <p className="text-muted-foreground">{t("退出应用不等于关闭观测；需停止记录时，请先点击“关闭观测”。")}</p>
        {current?.running && <div className="grid min-w-0 gap-2 rounded-lg border bg-muted/30 p-3 text-xs">
          <div>{t("状态：运行中 · 已写入 {written} 条 · 写入异常 {errors} 次", { written: current.writtenRequests, errors: current.storageErrors })}</div>
          <div className="break-all">{t("本机观测入口：")}{current.proxyUrl}</div>
          <div className="break-all">{t("公开证书：")}{current.certificatePath}</div>
          <div>{t("公开证书仅供客户端进程信任，不修改 auth.json、系统代理或系统证书库。Windows 签名密钥以当前用户 DPAPI 密文保留。")}</div>
        </div>}
        {error && <p role="alert" className="break-words text-destructive">{error instanceof Error ? error.message : "观测操作失败"}</p>}
      </CardContent>
    </Card>
  );
}
