"use client";

import { LanguageSwitcher } from "@/components/layout/language-switcher";
import { DisclaimerTicker } from "@/components/layout/disclaimer-ticker";
import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { AppearanceTabContent } from "@/app/settings/components/appearance-tab-content";
import { AboutCodexManagerCard } from "@/app/settings/components/general-tab-cards";
import { ProxySettingsCard } from "@/app/settings/components/proxy-settings-card";
import { DesktopDiagnosticsCard } from "@/app/settings/components/desktop-diagnostics-card";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { applyAppearancePreset, normalizeAppearancePreset } from "@/lib/appearance";
import { appClient } from "@/lib/api/app-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import type { AppSettings } from "@/types";

const SETTINGS_QUERY_KEY = ["app-settings-snapshot"] as const;

interface SettingSwitchRowProps {
  label: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}

/// 渲染个人版布尔设置行；每次只提交单个字段，失败时由页面保留服务端原状态。
function SettingSwitchRow(props: SettingSwitchRowProps) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-border/60 py-4 last:border-b-0">
      <div className="min-w-0 space-y-1">
        <Label>{props.label}</Label>
        <p className="text-xs text-muted-foreground">{props.description}</p>
      </div>
      <Switch
        aria-label={props.label}
        checked={props.checked}
        disabled={props.disabled}
        onCheckedChange={props.onChange}
      />
    </div>
  );
}

// 设置按职责分为五个标签；各功能沿用原持久化与权限边界，不在通用页重复挂载。
export default function SettingsPage() {
  const { t } = useI18n();
  const { theme, setTheme } = useTheme();
  const queryClient = useQueryClient();
  const setAppSettings = useAppStore((state) => state.setAppSettings);
  const { isDesktopRuntime, canAccessManagementRpc } = useRuntimeCapabilities();
  const settingsQuery = useQuery({
    queryKey: SETTINGS_QUERY_KEY,
    queryFn: appClient.getSettings,
    enabled: canAccessManagementRpc,
  });
  usePageTransitionReady("/settings/", !settingsQuery.isLoading);

  useEffect(() => {
    if (settingsQuery.data) setAppSettings(settingsQuery.data);
  }, [setAppSettings, settingsQuery.data]);

  const updateSettings = useMutation({
    mutationFn: (patch: Partial<AppSettings>) => appClient.setSettings(patch),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(SETTINGS_QUERY_KEY, snapshot);
      setAppSettings(snapshot);
      toast.success(t("设置已保存"));
    },
    onError: (error) => toast.error(getAppErrorMessage(error)),
  });

  const snapshot = settingsQuery.data;
  if (!snapshot) {
    return <div className="flex h-64 items-center justify-center text-muted-foreground">{t("加载配置中...")}</div>;
  }

  const change = (patch: Partial<AppSettings>) => updateSettings.mutate(patch);

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-muted-foreground">{t("管理应用行为、网络出口、费用显示和技术诊断")}</p>
      </div>
      <Tabs defaultValue="general" className="w-full">
        <TabsList className="glass-card mb-6 grid h-auto w-full grid-cols-2 gap-1 rounded-lg p-1 lg:flex lg:h-11 lg:w-fit">
          <TabsTrigger value="general">{t("通用")}</TabsTrigger>
          <TabsTrigger value="appearance">{t("外观")}</TabsTrigger>
          <TabsTrigger value="network">{t("网络")}</TabsTrigger>
          {isDesktopRuntime && <TabsTrigger value="diagnostics">{t("诊断")}</TabsTrigger>}
          <TabsTrigger value="about">{t("关于")}</TabsTrigger>
        </TabsList>
        <TabsContent value="general" className="space-y-6">
          <Card className="glass-card shadow-sm">
            <CardHeader><CardTitle className="text-base">{t("基础设置")}</CardTitle></CardHeader>
            <CardContent>
              <div className="flex flex-wrap items-center justify-between gap-4 border-b border-border/60 py-4">
                <Label>{t("语言")}</Label>
                <LanguageSwitcher />
              </div>
              <SettingSwitchRow label={t("自动检查更新")} description={t("启动完成后在后台检查更新")} checked={snapshot.updateAutoCheck} onChange={(value) => change({ updateAutoCheck: value })} />
              <SettingSwitchRow label={t("开机自动启动")} description={t("系统登录后自动启动桌面端")} checked={snapshot.autoStartEnabled} disabled={!snapshot.autoStartSupported} onChange={(value) => change({ autoStartEnabled: value })} />
              <SettingSwitchRow label={t("启动时显示主界面")} description={t("关闭后从托盘按需打开主界面")} checked={snapshot.showMainWindowOnStartup} onChange={(value) => change({ showMainWindowOnStartup: value })} />
              <SettingSwitchRow label={t("关闭时最小化到托盘")} description={t("保留本机接入和会话分流后台运行")} checked={snapshot.closeToTrayOnClose} disabled={!snapshot.closeToTraySupported} onChange={(value) => change({ closeToTrayOnClose: value })} />
            </CardContent>
          </Card>
        </TabsContent>
        <TabsContent value="network"><ProxySettingsCard canManage={canAccessManagementRpc} /></TabsContent>
        {isDesktopRuntime && <TabsContent value="diagnostics"><DesktopDiagnosticsCard t={t} /></TabsContent>}
        <TabsContent value="about" className="space-y-4"><AboutCodexManagerCard t={t} /><div className="glass-card rounded-xl border border-border/60 p-5"><DisclaimerTicker compact /></div></TabsContent>
        <TabsContent value="appearance">
          <AppearanceTabContent
            t={t}
            theme={theme}
            appearancePreset={normalizeAppearancePreset(snapshot.appearancePreset)}
            isDesktopRuntime={isDesktopRuntime}
            zoomFactor={snapshot.zoomFactor}
            onThemeChange={(value) => { setTheme(value); change({ theme: value }); }}
            onAppearancePresetChange={(value) => { applyAppearancePreset(value); change({ appearancePreset: value }); }}
            onZoomFactorChange={(value) => change({ zoomFactor: value })}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
