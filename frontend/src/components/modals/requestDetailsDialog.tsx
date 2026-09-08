"use client";

import { useQuery } from "@tanstack/react-query";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { NetworkBodyViewer, NetworkHeaders } from "./networkDetailViews";
import { serviceClient } from "@/lib/api/service-client";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";
import type { RequestLog } from "@/types";

// 请求标识作为标签树的重建键，切换记录时清理筛选和格式状态；查询仍按服务及记录隔离缓存。
export function RequestDetailsDialog({ log, onClose }: { log: RequestLog | null; onClose: () => void }) {
  const { t } = useI18n();
  const address = useAppStore((state) => state.serviceStatus.addr);
  const details = useQuery({
    queryKey: ["request-details", address, log?.id, log?.traceId],
    queryFn: () => serviceClient.requestDetails(log!.traceId),
    enabled: Boolean(log?.traceId),
    retry: 1,
  });
  const general = log ? {
    URL: log.upstreamUrl || log.path || log.requestPath,
    Method: log.method,
    Status: log.statusCode ?? "—",
    Protocol: log.requestType || "—",
    Duration: log.durationMs == null ? "—" : `${log.durationMs} ms`,
    TTFB: log.firstResponseMs == null ? "—" : `${log.firstResponseMs} ms`,
  } : undefined;
  return <Dialog open={Boolean(log)} onOpenChange={(open) => { if (!open) onClose(); }}>
    <DialogContent className="flex h-[86dvh] max-h-[94dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-[94vw] md:max-w-[min(94vw,1280px)]">
      <DialogHeader className="border-b border-border bg-muted/20 px-5 py-4 pr-14">
        <DialogTitle className="flex items-center gap-3 text-sm"><span className="rounded border border-primary/20 bg-primary/5 px-2 py-1 font-mono text-xs text-primary">{log?.method}</span>{t("请求详情")}<span className={`text-xs tabular-nums ${log?.statusCode && log.statusCode >= 400 ? "text-destructive" : "text-muted-foreground"}`}>{log?.statusCode ?? "—"}</span></DialogTitle>
        <DialogDescription className="truncate font-mono text-xs" title={log?.upstreamUrl || log?.path}>{log?.upstreamUrl || log?.path || log?.requestPath || t("客户端完成事件")}</DialogDescription>
      </DialogHeader>
      {details.isPending && log?.traceId ? <p role="status" className="p-5 text-sm">{t("加载中...")}</p> : details.isError ? <p role="alert" className="p-5 text-sm text-destructive">{details.error.message}</p> : <>
        {!details.data && <p role="status" className="border-b border-border bg-amber-500/5 px-5 py-3 text-xs text-muted-foreground">{t("仅收到客户端完成事件；该记录没有网络报文，需重新接入后采集新请求。")}</p>}
        {log?.error && <p className="max-h-20 overflow-auto break-words border-b border-destructive/20 bg-destructive/5 px-5 py-2 text-xs text-destructive">{log.error}</p>}
        <Tabs key={`${address}:${log?.id}`} defaultValue="headers" className="min-h-0 flex-1 gap-0">
          <TabsList variant="line" className="h-11 w-full shrink-0 justify-start overflow-x-auto rounded-none border-b border-border px-3">
            <TabsTrigger value="headers">{t("标头")}</TabsTrigger><TabsTrigger value="payload">{t("请求体")}</TabsTrigger><TabsTrigger value="response">{t("响应体")}</TabsTrigger><TabsTrigger value="events">{t("事件流")}</TabsTrigger>
          </TabsList>
          <TabsContent value="headers" className="min-h-0 flex-1 overflow-auto">
            <NetworkHeaders title={t("概览")} value={general} />
            <NetworkHeaders title={t("响应头")} value={details.data?.response.headers} />
            <NetworkHeaders title={t("请求头")} value={details.data?.request.headers} />
          </TabsContent>
          <TabsContent value="payload" className="flex min-h-0 flex-1 flex-col overflow-hidden"><NetworkBodyViewer value={details.data?.request.body} /></TabsContent>
          <TabsContent value="response" className="flex min-h-0 flex-1 flex-col overflow-hidden"><NetworkBodyViewer value={details.data?.response.body} /></TabsContent>
          <TabsContent value="events" className="flex min-h-0 flex-1 flex-col overflow-hidden"><NetworkBodyViewer value={details.data?.response.events} /></TabsContent>
        </Tabs>
      </>}
    </DialogContent>
  </Dialog>;
}
