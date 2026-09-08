"use client";

import { useMemo, useState } from "react";
import { Copy, Search, WrapText } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/lib/i18n/provider";

// 只格式化完整 JSON；SSE、普通文本和不完整流保持原文，避免改变采集到的报文语义。
export function formatNetworkBody(value: unknown, pretty: boolean): string | undefined {
  if (value == null) return undefined;
  if (typeof value !== "string") return JSON.stringify(value, null, pretty ? 2 : undefined);
  if (!pretty) return value;
  try { return JSON.stringify(JSON.parse(value), null, 2); }
  catch { return value; }
}

// 单个标签独占可滚动正文区；搜索只过滤显示行，复制始终保留当前格式的完整脱敏正文。
export function NetworkBodyViewer({ value }: { value: unknown }) {
  const { t } = useI18n();
  const [pretty, setPretty] = useState(true);
  const [wrap, setWrap] = useState(false);
  const [search, setSearch] = useState("");
  const text = useMemo(() => formatNetworkBody(value, pretty), [value, pretty]);
  const lines = useMemo(() => (text ?? "").split("\n").map((line, index) => ({ line, number: index + 1 })).filter(({ line }) => !search || line.toLowerCase().includes(search.toLowerCase())), [text, search]);
  // 剪贴板错误显式提示；仅复制已由服务端脱敏的详情，不重新读取认证存储。
  async function copyBody() {
    if (text === undefined) return;
    try { await navigator.clipboard.writeText(text); toast.success(t("已复制到剪贴板")); }
    catch { toast.error(t("复制失败")); }
  }
  return <div className="flex min-h-0 flex-1 flex-col">
    <div className="flex flex-wrap items-center gap-2 border-b border-border bg-muted/20 px-4 py-2">
      <div className="relative min-w-40 flex-1"><Search className="absolute left-2 top-2.5 size-3.5 text-muted-foreground" /><Input aria-label={t("筛选正文行")} placeholder={t("筛选正文行")} value={search} onChange={(event) => setSearch(event.target.value)} className="h-8 pl-7 text-xs" /></div>
      <Button variant="ghost" size="sm" aria-pressed={pretty} onClick={() => setPretty(!pretty)}>{t("格式化")}</Button>
      <Button variant="ghost" size="sm" aria-pressed={wrap} onClick={() => setWrap(!wrap)}><WrapText className="mr-1 size-4" />{t("自动换行")}</Button>
      <Button variant="ghost" size="sm" disabled={text === undefined} onClick={() => void copyBody()}><Copy className="mr-1 size-4" />{t("复制")}</Button>
    </div>
    <div className="min-h-0 flex-1 overflow-auto py-3 font-mono text-xs leading-6">
      {text === undefined ? <p className="px-4 text-muted-foreground">{t("未采集到该请求的网络报文")}</p> : lines.length === 0 ? <p className="px-4 text-muted-foreground">{t("无匹配结果")}</p> : lines.map(({ line, number }) => <div key={number} className="flex min-w-full hover:bg-muted/30"><span aria-hidden="true" className="sticky left-0 w-14 shrink-0 select-none border-r border-border/50 bg-background pr-3 text-right text-muted-foreground/60">{number}</span><pre className={`m-0 min-w-0 flex-1 px-4 ${wrap ? "whitespace-pre-wrap break-all" : "whitespace-pre"}`}>{line || " "}</pre></div>)}
    </div>
  </div>;
}

// Headers 使用键值行而非 JSON 文本；浏览器不解析成 HTML，报文中的任意字符串保持纯文本。
export function NetworkHeaders({ title, value }: { title: string; value: unknown }) {
  const { t } = useI18n();
  return <details open className="border-b border-border/60"><summary className="cursor-pointer bg-muted/25 px-4 py-2.5 text-xs font-semibold">{title}</summary><dl className="px-5 py-3 text-xs">{value && typeof value === "object" ? Object.entries(value).map(([name, entry]) => <div key={name} className="grid grid-cols-[minmax(120px,30%)_1fr] gap-4 py-1.5"><dt className="break-all font-medium text-muted-foreground">{name}</dt><dd className="min-w-0 whitespace-pre-wrap break-all font-mono">{typeof entry === "string" ? entry : JSON.stringify(entry)}</dd></div>) : <p className="text-muted-foreground">{t("未采集到该请求的网络报文")}</p>}</dl></details>;
}
