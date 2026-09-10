"use client";

import { useDeferredValue, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, ArrowRightLeft, Trash2, MessagesSquare, Copy } from "lucide-react";
import { copyTextToClipboard } from "@/lib/utils/clipboard";
import { toast } from "sonner";
import { DropdownMenuContent, DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { ContextMenu } from "@base-ui/react/context-menu";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { sessionRoutingClient, type RoutingSession } from "@/lib/api/sessionRoutingClient";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";

// 会话页只管理路由记录；查询隔离服务地址，后台页停止轮询，写操作等待后端确认。
export default function SessionsPage() {
  const { t } = useI18n();
  const service = useAppStore((state) => state.serviceStatus);
  const active = useDesktopPageActive("/sessions");
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [page, setPage] = useState(1);
  const [project, setProject] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [resetTarget, setResetTarget] = useState<RoutingSession | null>(null);
  const [switchTarget, setSwitchTarget] = useState<RoutingSession | null>(null);
  const [accountId, setAccountId] = useState("");
  const sessions = useQuery({
    queryKey: ["routingSessions", service.addr, page, deferredSearch, project],
    queryFn: () => sessionRoutingClient.list(page, deferredSearch, project),
    enabled: service.connected && active,
    refetchInterval: active && service.connected ? 5000 : false,
  });
  // 刷新会话页与账号绑定计数，避免写操作后两个页面显示不同快照。
  async function refreshBindings() {
    await Promise.all([queryClient.invalidateQueries({queryKey:["routingSessions"]}),queryClient.invalidateQueries({queryKey:["sessionRouting"]})]);
  }
  // 删除与原重置共用后端原子删除接口，记录和绑定同表清除；成功后退回有效页并清空旧选中项。
  const reset = useMutation({ mutationFn: sessionRoutingClient.reset, onSuccess: async () => {
    setSelectedId(null);
    if (sessions.data?.items.length === 1 && page > 1) setPage(page - 1);
    await refreshBindings();
  }, onError: (error) => toast.error(getAppErrorMessage(error)) });
  const change = useMutation({
    mutationFn: async () => {
      if (!switchTarget || !accountId) throw new Error(t("请选择账号"));
      const result = await sessionRoutingClient.switchAccount(switchTarget.sessionId,accountId);
      if (!result.ok) throw new Error(t("会话记录已不存在"));
    },
    onSuccess: async () => { setSwitchTarget(null); await refreshBindings(); toast.success(t("已安排切换，将在当前请求结束后生效")); },
    onError: (error) => toast.error(getAppErrorMessage(error)),
  });
  // 复制失败通过统一错误提示暴露，不把尚未写入剪贴板的值报告为成功。
  async function copyValue(value: string) {
    try { await copyTextToClipboard(value); toast.success(t("已复制到剪贴板")); }
    catch (error) { toast.error(getAppErrorMessage(error)); }
  }
  const selected = sessions.data?.items.find((session) => session.sessionId === selectedId) ?? sessions.data?.items[0];
  const totalPages = Math.max(1,Math.ceil((sessions.data?.total ?? 0)/50));
  const reasons: Record<string,string> = {initial_assignment:t("首次分配"),manual_switch:t("手动切换"),account_deleted:t("账号已删除"),quota_exhausted:t("额度耗尽"),account_unavailable:t("账号不可用"),no_available_account:t("暂无可用账号"),routing_disabled:t("分流已关闭")};
  return <div className="space-y-3">
    <div className="flex flex-wrap items-center gap-2 rounded-lg border border-border/50 bg-background/70 p-3">
      <Input className="min-w-48 flex-1 lg:max-w-sm" aria-label={t("搜索会话标题或 ID")} placeholder={t("搜索会话标题或 ID")} value={search} onChange={(event) => { setSearch(event.target.value);setPage(1); }} />
      <select className="h-9 max-w-xs rounded-md border border-input bg-background px-3 text-sm" aria-label={t("项目筛选")} value={project} onChange={(event) => {setProject(event.target.value);setPage(1);}}><option value="">{t("所有项目")}</option>{sessions.data?.projects?.map((path) => <option key={path} value={path}>{path.split(/[\\/]/).filter(Boolean).pop() || path}</option>)}</select>
      <span className="text-xs text-muted-foreground">{t("共")} {sessions.data?.total ?? "—"}</span>
      <Button className="ml-auto" variant="outline" disabled={!service.connected || sessions.isFetching} onClick={() => void sessions.refetch()}><RefreshCw className="mr-2 size-4" />{t("刷新")}</Button>
      <p className="w-full text-[11px] text-muted-foreground">{t("仅显示已接入会话；重置绑定不删除 Codex 聊天内容。")}</p>
    </div>
    {sessions.error && <p role="alert" className="text-sm text-destructive">{getAppErrorMessage(sessions.error)}</p>}
    {sessions.data?.metadataWarning && <p role="status" className="text-xs text-muted-foreground">{sessions.data.metadataWarning}</p>}
    <div className="grid min-h-[360px] overflow-hidden rounded-lg border border-border/50 bg-background/70 lg:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
      <section className="min-w-0 overflow-hidden border-b border-border/50 lg:border-r lg:border-b-0">
        <div className="border-b border-border/50 px-4 py-3 text-xs font-medium text-muted-foreground">{t("最近会话")}</div>
        <div className="max-h-[62vh] overflow-y-auto p-1.5">
          {sessions.data?.items.map((session) => <ContextMenu.Root key={session.sessionId}><ContextMenu.Trigger onContextMenu={() => setSelectedId(session.sessionId)} className={`group flex items-center rounded-md ${selected?.sessionId === session.sessionId ? "bg-primary/10 ring-1 ring-inset ring-primary/15" : "hover:bg-muted/40"}`}>
            <button type="button" className="min-w-0 flex-1 px-2.5 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-pressed={selected?.sessionId === session.sessionId} title={`${session.title || ""}\n${session.sessionId}`} onClick={() => setSelectedId(session.sessionId)}>
              <span className="block truncate text-sm font-medium">{session.title?.trim() || session.sessionId.slice(0,8)}</span>
              <span className="mt-1 flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground"><span className={`size-1.5 shrink-0 rounded-full ${session.status === "active" ? "bg-green-500" : "bg-amber-500"}`} /><span className="truncate">{session.accountLabel ?? t("待分配")}</span><span className="ml-auto shrink-0">{t(session.status === "active" ? "正常" : session.status === "unbound" ? "未分流" : "待切换")}</span></span>
            </button>
          </ContextMenu.Trigger>
            <DropdownMenuContent>
              <DropdownMenuItem onClick={() => setSelectedId(session.sessionId)}>{t("详情")}</DropdownMenuItem>
              <DropdownMenuItem onClick={() => {setSwitchTarget(session);setAccountId("");}}><ArrowRightLeft className="size-4" />{t("切换账号")}</DropdownMenuItem>
              <DropdownMenuItem disabled={reset.isPending} onClick={() => setResetTarget(session)}><Trash2 className="size-4" />{t("删除会话记录")}</DropdownMenuItem>
            </DropdownMenuContent>
          </ContextMenu.Root>)}
          {!sessions.data?.items.length && <p role="status" className="py-12 text-center text-sm text-muted-foreground">{t(sessions.isFetching ? "加载中..." : "暂无会话记录")}</p>}
        </div>
      </section>
      <section className="min-w-0 p-4 sm:p-5">
        {selected ? <>
          <div className="mb-5 flex items-start gap-3"><span className="rounded-lg bg-primary/10 p-2 text-primary"><MessagesSquare className="size-5" /></span><div className="min-w-0"><h2 className="line-clamp-2 break-words text-base font-semibold">{selected.title?.trim() || selected.sessionId.slice(0,8)}</h2><div className="mt-1 flex items-center gap-1 text-xs text-muted-foreground"><p className="min-w-0 truncate" title={selected.projectPath || undefined}>{selected.projectPath || "—"}</p>{selected.projectPath && <Button variant="ghost" size="icon" className="size-6 shrink-0" aria-label={t("复制项目路径")} onClick={() => void copyValue(selected.projectPath!)}><Copy className="size-3" /></Button>}</div></div></div>
          <div className="mb-5 flex flex-wrap gap-2"><Button variant="outline" size="sm" onClick={() => {setSwitchTarget(selected);setAccountId("");}}><ArrowRightLeft className="size-3.5" />{t("切换账号")}</Button><Button variant="ghost" size="sm" disabled={reset.isPending} onClick={() => setResetTarget(selected)}><Trash2 className="size-3.5" />{t("删除会话记录")}</Button></div>
          <dl className="grid gap-x-6 gap-y-4 text-sm xl:grid-cols-2">{[
            ["会话 ID",selected.sessionId], ["绑定账号",selected.accountLabel ?? t("待分配")],
            ["状态",t(selected.status === "active" ? "正常" : selected.status === "unbound" ? "未分流" : "待切换")],
            ["最近活动",new Date(selected.lastUsedAt*1000).toLocaleString()], ["首次连接",new Date(selected.createdAt*1000).toLocaleString()],
            ["迁移原因",reasons[selected.reason] ?? selected.reason],
          ].map(([label,value]) => <div key={label} className="min-w-0 space-y-1.5"><dt className="text-xs text-muted-foreground">{t(label)}</dt><dd className="flex items-start gap-1 text-xs leading-5"><span className="min-w-0 break-all select-text">{value}</span>{label === "会话 ID" && <Button variant="ghost" size="icon" className="size-6 shrink-0" aria-label={t("复制会话 ID")} onClick={() => void copyValue(value)}><Copy className="size-3" /></Button>}</dd></div>)}</dl>
          {selected.requestedAccountLabel && <p className="mt-4 text-sm text-primary">→ {selected.requestedAccountLabel}</p>}
        </> : <div className="flex h-full items-center justify-center text-sm text-muted-foreground">{t("暂无会话记录")}</div>}
      </section>
    </div>
    <div className="flex items-center justify-end gap-3 text-xs"><Button variant="outline" disabled={page<=1} onClick={() => setPage(page-1)}>{t("上一页")}</Button><span>{page} / {totalPages}</span><Button variant="outline" disabled={page>=totalPages} onClick={() => setPage(page+1)}>{t("下一页")}</Button></div>
    <ConfirmDialog open={Boolean(resetTarget)} onOpenChange={(open) => {if(!open)setResetTarget(null);}} title={t("删除会话记录")} confirmVariant="destructive" description={t("删除此分流记录，下次连接重新分配账号；不会删除聊天内容。当前请求保持原账号直到结束。")}
      onConfirm={async () => {if(resetTarget)await reset.mutateAsync(resetTarget.sessionId);}} />
    <Dialog open={Boolean(switchTarget)} onOpenChange={(open) => {if(!open && !change.isPending)setSwitchTarget(null);}}>
      <DialogContent><DialogHeader><DialogTitle>{t("切换账号")}</DialogTitle><DialogDescription>{t("当前请求结束后切换连接，不重放已经发送的请求。")}</DialogDescription></DialogHeader>
        <select className="h-10 rounded-md border border-input bg-background px-3 text-sm" aria-label={t("请选择账号")} value={accountId} disabled={change.isPending} onChange={(event) => setAccountId(event.target.value)}>
          <option value="">{t("请选择账号")}</option>{sessions.data?.accounts.map((account) => <option key={account.id} value={account.id}>{account.label}</option>)}
        </select>
        <Button disabled={!accountId || change.isPending} onClick={() => change.mutate()}>{t("确定")}</Button>
      </DialogContent>
    </Dialog>
  </div>;
}
