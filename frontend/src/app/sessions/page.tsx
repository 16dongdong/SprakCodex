"use client";

import { useDeferredValue, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, RotateCcw, ArrowRightLeft } from "lucide-react";
import { toast } from "sonner";
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
  const [resetTarget, setResetTarget] = useState<RoutingSession | null>(null);
  const [switchTarget, setSwitchTarget] = useState<RoutingSession | null>(null);
  const [accountId, setAccountId] = useState("");
  const sessions = useQuery({
    queryKey: ["routingSessions", service.addr, page, deferredSearch],
    queryFn: () => sessionRoutingClient.list(page, deferredSearch),
    enabled: service.connected && active,
    refetchInterval: active && service.connected ? 5000 : false,
  });
  // 刷新会话页与账号绑定计数，避免写操作后两个页面显示不同快照。
  async function refreshBindings() {
    await Promise.all([queryClient.invalidateQueries({queryKey:["routingSessions"]}),queryClient.invalidateQueries({queryKey:["sessionRouting"]})]);
  }
  const reset = useMutation({ mutationFn: sessionRoutingClient.reset, onSuccess: refreshBindings, onError: (error) => toast.error(getAppErrorMessage(error)) });
  const change = useMutation({
    mutationFn: async () => {
      if (!switchTarget || !accountId) throw new Error(t("请选择账号"));
      const result = await sessionRoutingClient.switchAccount(switchTarget.sessionId,accountId);
      if (!result.ok) throw new Error(t("会话记录已不存在"));
    },
    onSuccess: async () => { setSwitchTarget(null); await refreshBindings(); toast.success(t("已安排切换，将在当前请求结束后生效")); },
    onError: (error) => toast.error(getAppErrorMessage(error)),
  });
  const totalPages = Math.max(1,Math.ceil((sessions.data?.total ?? 0)/50));
  const reasons: Record<string,string> = {initial_assignment:t("首次分配"),manual_switch:t("手动切换"),account_deleted:t("账号已删除"),quota_exhausted:t("额度耗尽"),account_unavailable:t("账号不可用"),no_available_account:t("暂无可用账号"),routing_disabled:t("分流已关闭")};
  return <div className="space-y-4">
    <div className="glass-card flex flex-wrap items-center gap-3 rounded-xl border border-border/60 p-4">
      <Input className="max-w-md" aria-label={t("搜索会话 ID")} placeholder={t("搜索会话 ID")} value={search} onChange={(event) => { setSearch(event.target.value);setPage(1); }} />
      <span className="text-xs text-muted-foreground">{t("共")} {sessions.data?.total ?? "—"}</span>
      <Button className="ml-auto" variant="outline" disabled={!service.connected || sessions.isFetching} onClick={() => void sessions.refetch()}><RefreshCw className="mr-2 size-4" />{t("刷新")}</Button>
      <p className="w-full text-xs text-muted-foreground">{t("仅显示已接入会话；重置绑定不删除 Codex 聊天内容。")}</p>
    </div>
    {sessions.error && <p role="alert" className="text-sm text-destructive">{getAppErrorMessage(sessions.error)}</p>}
    <div className="glass-card overflow-x-auto rounded-xl border border-border/60">
      <table className="w-full min-w-[920px] text-left text-xs">
        <thead className="border-b border-border bg-muted/20 text-muted-foreground"><tr>{["会话 ID","绑定账号","状态","最近活动","首次连接","迁移原因","操作"].map((title) => <th key={title} className="px-4 py-3 font-medium">{t(title)}</th>)}</tr></thead>
        <tbody>{sessions.data?.items.map((session) => <tr key={session.sessionId} className="border-b border-border/40 last:border-0 hover:bg-muted/20">
          <td className="max-w-56 truncate px-4 py-4 font-mono" title={session.sessionId}>{session.sessionId}</td>
          <td className="max-w-52 px-4 py-4"><div className="truncate" title={session.accountLabel ?? ""}>{session.accountLabel ?? t("待分配")}</div>{session.requestedAccountLabel && <div className="mt-1 truncate text-primary">→ {session.requestedAccountLabel}</div>}</td>
          <td className="whitespace-nowrap px-4 py-4">{t(session.status === "active" ? "正常" : session.status === "unbound" ? "未分流" : "待切换")}</td>
          <td className="whitespace-nowrap px-4 py-4 tabular-nums">{new Date(session.lastUsedAt*1000).toLocaleString()}</td>
          <td className="whitespace-nowrap px-4 py-4 tabular-nums">{new Date(session.createdAt*1000).toLocaleString()}</td>
          <td className="px-4 py-4">{reasons[session.reason] ?? session.reason}</td>
          <td className="px-4 py-4"><div className="flex gap-2"><Button size="sm" variant="ghost" onClick={() => {setSwitchTarget(session);setAccountId("");}}><ArrowRightLeft className="mr-1 size-3.5" />{t("切换账号")}</Button><Button size="sm" variant="ghost" onClick={() => setResetTarget(session)}><RotateCcw className="mr-1 size-3.5" />{t("重置绑定")}</Button></div></td>
        </tr>)}</tbody>
      </table>
      {!sessions.data?.items.length && <p role="status" className="py-12 text-center text-sm text-muted-foreground">{t(sessions.isFetching ? "加载中..." : "暂无会话记录")}</p>}
    </div>
    <div className="flex items-center justify-end gap-3 text-sm"><Button variant="outline" disabled={page<=1} onClick={() => setPage(page-1)}>{t("上一页")}</Button><span>{page} / {totalPages}</span><Button variant="outline" disabled={page>=totalPages} onClick={() => setPage(page+1)}>{t("下一页")}</Button></div>
    <ConfirmDialog open={Boolean(resetTarget)} onOpenChange={(open) => {if(!open)setResetTarget(null);}} title={t("重置绑定")} description={t("删除此分流记录，下次连接重新分配账号；不会删除聊天内容。当前请求保持原账号直到结束。")}
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
