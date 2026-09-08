"use client";

import { useQuery } from "@tanstack/react-query";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { serviceClient } from "@/lib/api/service-client";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";
import type { RequestLog } from "@/types";

// 原始文本不再次 JSON 转义；对象格式化后完整显示，空正文与未采集状态分开表达。
function formatBody(value: unknown, missing: string): string {
  if (value === undefined || value === null) return missing;
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

// 四个报文区域始终独立呈现；用量摘要不冒充网络详情，查询失败保留明确错误。
export function RequestDetailsDialog({ log, onClose }: { log: RequestLog | null; onClose: () => void }) {
  const { t } = useI18n();
  const address = useAppStore((state) => state.serviceStatus.addr);
  const details = useQuery({
    queryKey: ["request-details", address, log?.id],
    queryFn: () => serviceClient.requestDetails(log!.traceId),
    enabled: Boolean(log?.traceId),
    retry: 1,
  });
  const missing = t("未采集到该请求的网络报文");
  const sections = [
    { title: t("请求头"), value: details.data?.request.headers },
    { title: t("响应头"), value: details.data?.response.headers },
    { title: t("请求体"), value: details.data?.request.body },
    { title: t("响应体"), value: details.data?.response.body },
  ];
  return <Dialog open={Boolean(log)} onOpenChange={(open) => { if (!open) onClose(); }}>
    <DialogContent className="max-h-[94dvh] overflow-y-auto sm:max-w-[94vw] md:max-w-[min(94vw,1280px)]">
      <DialogHeader>
        <DialogTitle>{t("请求详情")}</DialogTitle>
        <DialogDescription>{log?.method} {log?.path || log?.requestPath || t("客户端完成事件")}</DialogDescription>
      </DialogHeader>
      {details.isFetching && log ? <p role="status">{t("加载中...")}</p> : details.isError ? <p role="alert" className="text-destructive">{details.error.message}</p> : <>
        {!details.data && <p role="status" className="text-sm text-amber-600">{t("仅收到客户端完成事件；该记录没有网络报文，需重新接入后采集新请求。")}</p>}
        {log?.error && <p className="break-words text-xs text-destructive">{log.error}</p>}
        <div className="grid min-w-0 gap-3 md:grid-cols-2">
          {sections.map((section, index) => <section key={section.title} className="min-w-0 space-y-2">
            <h3 className="text-sm font-medium">{section.title}</h3>
            <pre className={`${index < 2 ? "h-[18vh]" : "h-[36vh]"} overflow-auto whitespace-pre-wrap break-all rounded-lg border bg-muted/50 p-3 font-mono text-xs`}>{formatBody(section.value, missing)}</pre>
          </section>)}
        </div>
      </>}
    </DialogContent>
  </Dialog>;
}
