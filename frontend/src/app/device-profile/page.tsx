"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  Clock3,
  Fingerprint,
  Globe2,
  Languages,
  MapPin,
  Monitor,
  Network,
  RefreshCw,
  Server,
} from "lucide-react";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { observationClient } from "@/lib/api/observationClient";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";

type ProfileField = {
  label: string;
  value: string;
  icon: typeof Globe2;
  detail?: string;
};

// 毫秒时间戳只用于展示探针新鲜度；格式化遵循当前界面语言，不反向影响出口画像。
function formatProbeTime(value: number | null | undefined, locale: string): string {
  if (!value) return "—";
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
}

// 设备画像页只呈现自动探针的真实结果；本地时区和界面语言绝不作为出口字段来源。
export default function DeviceProfilePage() {
  const { t, locale } = useI18n();
  const runtime = useRuntimeCapabilities();
  const [switching, setSwitching] = useState(false);
  const observation = useQuery({
    queryKey: ["device-profile", "observation"],
    queryFn: observationClient.status,
    refetchInterval: 3000,
  });
  const status = observation.data;
  const enabled = status?.running ?? false;
  const fieldValue = (value: string | number | null | undefined) =>
    value === null || value === undefined || value === "" ? t("等待自动探针") : String(value);
  const fields: ProfileField[] = [
    {
      label: t("运行平台"),
      value: runtime.isDesktopRuntime ? "Windows Desktop" : "Web Runtime",
      icon: Monitor,
      detail: t("目标进程系统与架构保持原值"),
    },
    {
      label: t("OpenAI 边缘服务器"),
      value: fieldValue(status?.edgeServer),
      icon: Server,
      detail: fieldValue(status?.edgeLocation),
    },
    {
      label: t("出口国家或地区"),
      value: fieldValue(status?.egressCountry),
      icon: MapPin,
      detail: t("来自 OpenAI 边缘探针 loc 字段"),
    },
    {
      label: t("出口 IP"),
      value: fieldValue(status?.egressIp),
      icon: Network,
      detail: t("探针请求经过的实际代理出口"),
    },
    {
      label: t("出口 IANA 时区"),
      value: fieldValue(status?.egressTimezone),
      icon: Clock3,
      detail: t("同步到请求头与 Chromium/Node"),
    },
    {
      label: t("Windows 时区"),
      value: fieldValue(status?.egressWindowsTimezone),
      icon: Clock3,
      detail: t("实时发布给 DLL 的动态时区键"),
    },
    {
      label: t("出口语言"),
      value: fieldValue(status?.egressLocale),
      icon: Languages,
      detail: t("同步到 Locale API 与 OpenAI 请求头"),
    },
    {
      label: t("画像更新时间"),
      value: formatProbeTime(status?.profileUpdatedAt, locale),
      icon: RefreshCw,
      detail: t("仅在出口画像发生变化时更新"),
    },
    {
      label: t("最近探针时间"),
      value: formatProbeTime(status?.lastProbeAt, locale),
      icon: Activity,
      detail: t("每 {seconds} 秒请求一次 OpenAI 边缘", {
        seconds: status?.probeIntervalSeconds ?? 15,
      }),
    },
    {
      label: t("已记录请求"),
      value: fieldValue(status?.writtenRequests ?? 0),
      icon: Fingerprint,
      detail: t("画像同步运行期内完成写入的请求"),
    },
  ];

  // 开关以服务端返回状态为准；失败保持原状态并展示精确错误，不做乐观伪更新。
  const toggleSynchronization = async (checked: boolean) => {
    setSwitching(true);
    try {
      if (checked) await observationClient.start();
      else await observationClient.stop();
      await observation.refetch();
    } catch (error) {
      toast.error(getAppErrorMessage(error));
    } finally {
      setSwitching(false);
    }
  };

  return (
    <main className="h-full overflow-y-auto p-4 sm:p-6 lg:p-8">
      <div className="mx-auto grid max-w-6xl gap-5">
        <Card className="glass-card">
          <CardContent className="flex flex-col gap-4 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-start gap-3">
              <div className="rounded-xl border border-primary/20 bg-primary/10 p-2.5 text-primary">
                <Fingerprint className="size-5" />
              </div>
              <div>
                <div className="font-semibold">{t("出口画像自动同步")}</div>
                <div className="mt-1 text-sm text-muted-foreground">
                  {t("自动探测 OpenAI 实际边缘出口，并实时同步时区和语言到目标进程")}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-3 self-end sm:self-auto">
              <span className={enabled ? "text-sm text-emerald-500" : "text-sm text-muted-foreground"}>
                {enabled ? t("已启用") : t("已停用")}
              </span>
              <Switch
                checked={enabled}
                disabled={switching || observation.isLoading}
                onCheckedChange={toggleSynchronization}
                aria-label={t("出口画像自动同步")}
              />
            </div>
          </CardContent>
        </Card>

        {status?.lastProbeError ? (
          <div className="rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            <span className="font-medium">{t("最近探针失败")}：</span>
            {status.lastProbeError}
          </div>
        ) : null}

        <Card className="glass-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Globe2 className="size-4 text-primary" />
              {t("当前出口画像")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
              {fields.map(({ label, value, icon: Icon, detail }) => (
                <div
                  key={label}
                  className="rounded-xl border border-border/60 bg-background/30 p-4 transition-colors hover:bg-background/50"
                >
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Icon className="size-3.5 text-primary" />
                    {label}
                  </div>
                  <div className="mt-2 break-all font-medium">{value}</div>
                  {detail ? <div className="mt-1.5 text-xs text-muted-foreground">{detail}</div> : null}
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </main>
  );
}
