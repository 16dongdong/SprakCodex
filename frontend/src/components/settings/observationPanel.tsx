"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Radio, Loader2 } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { observationClient, observationQueryKey } from "@/lib/api/observationClient";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";

// 接入方式页展示会话级观测开关；服务地址参与缓存键，避免切换服务后混用监听地址。
// 后端错误保留可见，公开证书按进程信任，不要求安装系统根证书或复制登录文件。
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
        <p className="text-muted-foreground">{t("启用后，从本机「项目启动」打开新的 Codex 终端即可接入。已运行的终端不变；关闭观测前请结束接入的终端。每次重启需重新启用。")}</p>
        {current?.running && <div className="grid min-w-0 gap-2 rounded-lg border bg-muted/30 p-3 text-xs">
          <div>{t("状态：运行中 · 已写入 {written} 条 · 写入异常 {errors} 次", { written: current.writtenRequests, errors: current.storageErrors })}</div>
          <div className="break-all">{t("本机代理：")}{current.proxyUrl}</div>
          <div className="break-all">{t("公开证书：")}{current.certificatePath}</div>
          <div>{t("仅在子进程内信任临时证书；不修改 auth.json、系统代理或系统证书库。独立服务模式需在服务主机为客户端设置此代理和 CODEX_CA_CERTIFICATE。")}</div>
        </div>}
        {error && <p role="alert" className="break-words text-destructive">{error instanceof Error ? error.message : "观测操作失败"}</p>}
      </CardContent>
    </Card>
  );
}
