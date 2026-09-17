"use client";

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff, Network, Route, Save, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { appClient } from "@/lib/api/app-client";
import { observationClient } from "@/lib/api/observationClient";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";

// 代理地址只在提交时进入服务配置；输入草稿不提前改变正在进行的 OpenAI 请求。
export default function ProxyPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const settings = useQuery({
    queryKey: ["proxy", "settings"],
    queryFn: appClient.getSettings,
  });
  const [proxyUrl, setProxyUrl] = useState("");
  const [revealAddress, setRevealAddress] = useState(false);

  useEffect(() => {
    if (settings.data) setProxyUrl(settings.data.upstreamProxyUrl);
  }, [settings.data]);

  // 观测运行期持有独立连接池；配置变化后重建运行期，确保下一条连接不再复用旧系统代理。
  const applyProxy = useMutation({
    mutationFn: async ({ enabled, url }: { enabled: boolean; url: string }) => {
      const normalizedUrl = url.trim();
      if (enabled && !normalizedUrl) {
        throw new Error(t("开启代理前请先填写代理地址"));
      }
      const observation = await observationClient.status();
      const previousSettings = await appClient.getSettings();
      let settingsApplied = false;
      try {
        const nextSettings = await appClient.setSettings({
          upstreamProxyUrl: normalizedUrl,
          upstreamProxyEnabled: enabled,
        });
        settingsApplied = true;
        if (observation.running) {
          await observationClient.stop();
          await observationClient.start();
        }
        return nextSettings;
      } catch (error) {
        // 探针或连接池重建失败时恢复原配置和运行期，禁止留下“开关已开但观测已停”的半状态。
        if (settingsApplied) {
          await appClient.setSettings({
            upstreamProxyUrl: previousSettings.upstreamProxyUrl,
            upstreamProxyEnabled: previousSettings.upstreamProxyEnabled,
          });
          if (observation.running) {
            const currentObservation = await observationClient.status();
            if (currentObservation.running) await observationClient.stop();
            await observationClient.start();
          }
        }
        throw error;
      }
    },
    onSuccess: async (nextSettings) => {
      setProxyUrl(nextSettings.upstreamProxyUrl);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["proxy", "settings"] }),
        queryClient.invalidateQueries({ queryKey: ["device-profile", "observation"] }),
      ]);
      toast.success(t("代理设置已生效"));
    },
    onError: (error) => toast.error(getAppErrorMessage(error)),
  });

  const enabled = settings.data?.upstreamProxyEnabled ?? false;
  const busy = settings.isLoading || applyProxy.isPending;

  return (
    <main className="h-full overflow-y-auto p-4 sm:p-6 lg:p-8">
      <div className="mx-auto grid max-w-5xl gap-5">
        <Card className="glass-card">
          <CardContent className="flex flex-col gap-4 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-start gap-3">
              <div className="rounded-xl border border-primary/20 bg-primary/10 p-2.5 text-primary">
                <Network className="size-5" />
              </div>
              <div>
                <div className="font-semibold">{t("开启代理")}</div>
                <div className="mt-1 text-sm text-muted-foreground">
                  {enabled
                    ? t("目标进程和 OpenAI 探针当前使用自定义代理出口")
                    : t("当前沿用目标进程或系统代理出口")}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-3 self-end sm:self-auto">
              <span className={enabled ? "text-sm text-emerald-500" : "text-sm text-muted-foreground"}>
                {enabled ? t("已启用") : t("已停用")}
              </span>
              <Switch
                checked={enabled}
                disabled={busy}
                onCheckedChange={(checked) =>
                  applyProxy.mutate({ enabled: checked, url: proxyUrl })
                }
                aria-label={t("开启代理")}
              />
            </div>
          </CardContent>
        </Card>

        <Card className="glass-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Route className="size-4 text-primary" />
              {t("自定义代理出口")}
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-5">
            <div className="grid gap-2">
              <Label htmlFor="proxy-url">{t("代理地址")}</Label>
              <div className="flex gap-2">
                <div className="relative min-w-0 flex-1">
                  <Input
                    id="proxy-url"
                    type={revealAddress ? "text" : "password"}
                    value={proxyUrl}
                    onChange={(event) => setProxyUrl(event.target.value)}
                    placeholder="http://127.0.0.1:7890"
                    autoComplete="off"
                    className="pr-10 font-mono"
                  />
                  <button
                    type="button"
                    onClick={() => setRevealAddress((current) => !current)}
                    className="absolute inset-y-0 right-0 flex w-10 items-center justify-center text-muted-foreground hover:text-foreground"
                    aria-label={revealAddress ? t("隐藏代理地址") : t("显示代理地址")}
                  >
                    {revealAddress ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                  </button>
                </div>
                <Button
                  disabled={busy}
                  onClick={() => applyProxy.mutate({ enabled, url: proxyUrl })}
                >
                  <Save className="size-4" />
                  {t("保存")}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                {t("支持 HTTP CONNECT 与 SOCKS5 地址；启用后不会再使用系统代理。")}
              </p>
            </div>

            <div className="grid gap-3 sm:grid-cols-2">
              <div className="rounded-xl border border-border/60 bg-background/30 p-4">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <ShieldCheck className="size-3.5 text-primary" />
                  {t("当前路由")}
                </div>
                <div className="mt-2 font-medium">
                  {enabled ? t("自定义代理") : t("系统代理或原始出口")}
                </div>
              </div>
              <div className="rounded-xl border border-border/60 bg-background/30 p-4">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Network className="size-3.5 text-primary" />
                  {t("影响范围")}
                </div>
                <div className="mt-2 font-medium">{t("目标进程、OpenAI 请求与画像探针")}</div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </main>
  );
}
