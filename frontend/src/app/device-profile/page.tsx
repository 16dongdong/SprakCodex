"use client";
import { useState } from "react";
import { Fingerprint, Globe2, Monitor, ShieldCheck } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { observationClient } from "@/lib/api/observationClient";
import { useQuery } from "@tanstack/react-query";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { useI18n } from "@/lib/i18n/provider";

// 设备画像页只展示当前桌面运行时和出口画像的可观测字段，不写入系统设置。
export default function DeviceProfilePage() {
  const { t } = useI18n();
  const observation = useQuery({ queryKey: ["device-profile", "observation"], queryFn: observationClient.status, refetchInterval: 10000 });
  const runtime = useRuntimeCapabilities();
  const enabled = observation.data?.running ?? false;
  const [timezone, setTimezone] = useState("");
  const [locale, setLocale] = useState("");
  const fields = [
    [t("运行平台"), runtime.isDesktopRuntime ? "Windows Desktop" : "Web Runtime"],
    [t("出口时区"), observation.data?.egressTimezone || t("未启动出口同步")],
    [t("出口语言"), observation.data?.egressLocale || t("未启动出口同步")],
  ];
  return <main className="h-full overflow-y-auto p-4 sm:p-6 lg:p-8"><div className="mx-auto grid max-w-6xl gap-5"><div className="grid gap-5 md:grid-cols-3"><Card className="glass-card"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Fingerprint className="size-4 text-primary" />{t("身份画像")}</CardTitle></CardHeader><CardContent className="text-sm text-muted-foreground">{t("设备字段保持本机系统与架构，地区字段跟随出口节点")}</CardContent></Card><Card className="glass-card"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Globe2 className="size-4 text-primary" />{t("出口地区")}</CardTitle></CardHeader><CardContent className="text-sm text-muted-foreground">{t("时区、国家与语言使用同一出口画像")}</CardContent></Card><Card className="glass-card"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><ShieldCheck className="size-4 text-primary" />{t("同步状态")}</CardTitle></CardHeader><CardContent className="flex items-center justify-between gap-3"><span className={enabled ? "text-sm text-emerald-500" : "text-sm text-muted-foreground"}>{enabled ? t("已启用") : t("已停用")}</span><Switch checked={enabled} onCheckedChange={async (checked) => { try { if (checked) await observationClient.start(); else await observationClient.stop(); await observation.refetch(); } catch { /* 运行状态由后端保持，失败时不改变界面状态 */ } }} /></CardContent></Card></div><Card className="glass-card"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Monitor className="size-4 text-primary" />{t("当前画像字段")}</CardTitle></CardHeader><CardContent><div className="grid gap-3 sm:grid-cols-2">{fields.map(([label, value]) => <div key={label} className="rounded-xl border border-border/60 bg-background/30 p-4"><div className="text-xs text-muted-foreground">{label}</div>{label === t("出口时区") ? <Input value={timezone || String(value)} onChange={(event) => setTimezone(event.target.value)} className="mt-2 h-8" /> : label === t("出口语言") ? <Input value={locale || String(value)} onChange={(event) => setLocale(event.target.value)} className="mt-2 h-8" /> : <div className="mt-1 font-medium">{value}</div>}</div>)}</div></CardContent></Card></div></main>;
}
