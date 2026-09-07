'use client';

import { AlertTriangle, TerminalSquare } from "lucide-react";
import { useI18n } from "@/lib/i18n/provider";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert";
import { Card, CardContent } from "@/components/ui/card";
import {
  AdvancedRecoveryPanel,
  CurrentModeCard,
  DirectAccountCard,
  GatewayModeCard,
  ReloadAfterSwitchOption,
} from "./page-sections";
import {
  modeImpact,
  usePlatformModePageState,
} from "./use-platform-mode-state";
import type {
  CodexProfileAccountCandidate,
  CodexProfileApiKeyCandidate,
} from "@/types";

function formatTime(ts: number | null): string {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString();
}

function formatBytes(bytes: number | null | undefined): string {
  const value = typeof bytes === "number" && Number.isFinite(bytes) ? bytes : 0;
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${units[index]}`;
}

function keyLabel(key: CodexProfileApiKeyCandidate): string {
  return key.name || key.modelSlug || key.id;
}

function accountLabel(account: CodexProfileAccountCandidate): string {
  return account.groupName ? `${account.label} · ${account.groupName}` : account.label;
}

export default function PlatformModePage() {
  const { t } = useI18n();
  const state = usePlatformModePageState(t);
  const selectedApiKey = state.candidates.apiKeys.find(
    (item) => item.id === state.selectedApiKeyId,
  );
  const activeApiKey = state.status?.selectedApiKeyId
    ? state.candidates.apiKeys.find(
        (item) => item.id === state.status?.selectedApiKeyId,
      )
    : undefined;

  return (
    <main className="flex w-full flex-col gap-5">
      <Card className="routing-command-card glass-card overflow-hidden py-0 shadow-sm">
        <CardContent className="flex min-h-[80px] items-center gap-3 px-4 py-3 xl:min-h-[92px] xl:gap-4 xl:px-5 xl:py-4">
          <div className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary/10 text-primary xl:size-10">
            <TerminalSquare className="size-[18px] xl:size-5" />
          </div>
          <div className="min-w-0">
            <h1 className="text-xl font-semibold leading-tight tracking-[-0.02em] text-foreground">
              {t("Codex 接入方式")}
            </h1>
            <p className="mt-1 text-sm leading-5 text-muted-foreground xl:leading-6">
              {t("选择 Codex 直接连接 OpenAI，或通过 CodexManager 进行转发与管理。")}
            </p>
          </div>
        </CardContent>
      </Card>

      <Alert className="border-amber-500/30 bg-amber-500/10">
        <AlertTriangle className="size-4" />
        <AlertTitle>{t("写入位置说明")}</AlertTitle>
        <AlertDescription>
          {t("这里修改的是 codexmanager-service 所在机器的 Codex 配置目录，不一定是当前浏览器所在机器。")}
        </AlertDescription>
      </Alert>

      {state.mode === "web-gateway" ? (
        <Alert className="border-sky-500/30 bg-sky-500/10">
          <AlertTriangle className="size-4" />
          <AlertTitle>{t("Web / Docker 模式")}</AlertTitle>
          <AlertDescription>
            {t("当前页面会通过 /api/rpc 写入 codexmanager-service 进程可访问的 Codex profile；Docker 部署时请确认 CODEX_HOME 或挂载卷指向你希望 Codex CLI 使用的配置目录。")}
          </AlertDescription>
        </Alert>
      ) : null}

      {!state.isServiceReady ? (
        <Alert variant="destructive">
          <AlertTriangle className="size-4" />
          <AlertTitle>{t("服务未连接")}</AlertTitle>
          <AlertDescription>
            {t("当前运行环境无法访问管理 RPC，暂时不能读取或写入 Codex profile。")}
          </AlertDescription>
        </Alert>
      ) : null}

      {state.status?.warnings.length ? (
        <Alert className="border-amber-500/30 bg-amber-500/10">
          <AlertTriangle className="size-4" />
          <AlertTitle>{t("Profile 迁移警告")}</AlertTitle>
          <AlertDescription>{state.status.warnings[0]}</AlertDescription>
        </Alert>
      ) : null}

      <ReloadAfterSwitchOption
        t={t}
        enabled={state.reloadAfterSwitch}
        disabled={!state.isServiceReady || state.isMutating}
        onEnabledChange={state.setReloadAfterSwitch}
      />

      <div className="grid gap-5 lg:grid-cols-2 xl:grid-cols-[minmax(320px,0.9fr)_minmax(0,1.05fr)_minmax(0,1.05fr)]">
        <CurrentModeCard
          t={t}
          status={state.status}
          isGatewayActive={state.isGatewayActive}
          statusFetching={state.statusQuery.isFetching}
          candidatesFetching={state.candidatesQuery.isFetching}
          onRefresh={() => void state.refreshAll()}
          codexHome={state.status?.codexHome || "-"}
          activeAccountValue={state.activeAccountValue}
          activeKeyValue={state.activeKeyValue}
          activeApiKey={activeApiKey}
          lastAppliedAtLabel={formatTime(state.status?.lastAppliedAt ?? null)}
          modeDescription={modeImpact(state.status?.mode ?? null, t)}
        />

        <DirectAccountCard
          t={t}
          candidates={state.candidates.accounts}
          isLoading={state.candidatesQuery.isLoading}
          isServiceReady={state.isServiceReady}
          isMutating={state.isMutating}
          isDirectActive={state.isDirectActive}
          selectedAccountId={state.selectedAccountId}
          onSelectAccount={(value) => state.setSelectedAccountIdDraft(String(value || ""))}
          onApply={() => state.applyDirectMutation.mutate()}
          isPending={state.applyDirectMutation.isPending}
          reloadAfterSwitch={state.reloadAfterSwitch}
          accountLabel={accountLabel}
        />

        <GatewayModeCard
          t={t}
          candidates={state.candidates.apiKeys}
          isLoading={state.candidatesQuery.isLoading}
          isServiceReady={state.isServiceReady}
          isMutating={state.isMutating}
          isGatewayActive={state.isGatewayActive}
          selectedApiKeyId={state.selectedApiKeyId}
          onSelectApiKey={(value) => state.setSelectedApiKeyIdDraft(String(value || ""))}
          gatewayBaseUrl={state.gatewayBaseUrl}
          supportsWebsockets={state.supportsWebsockets}
          onSupportsWebsocketsChange={state.setSupportsWebsocketsDraft}
          onApply={() => state.applyGatewayMutation.mutate()}
          isPending={state.applyGatewayMutation.isPending}
          selectedApiKey={selectedApiKey}
          reloadAfterSwitch={state.reloadAfterSwitch}
          keyLabel={keyLabel}
        />
      </div>

      <AdvancedRecoveryPanel
        t={t}
        status={state.status}
        isServiceReady={state.isServiceReady}
        isMutating={state.isMutating}
        codexHomeInput={state.codexHomeInput}
        latestHistoryRepair={state.latestHistoryRepair}
        formatBytes={formatBytes}
        onRepairHistory={() => state.repairHistoryMutation.mutate()}
        onPruneHistoryBackups={() => state.pruneHistoryBackupsMutation.mutate()}
        onRestore={() => state.restoreMutation.mutate()}
        saveConfigPending={state.saveConfigMutation.isPending}
        restorePending={state.restoreMutation.isPending}
        repairHistoryPending={state.repairHistoryMutation.isPending}
        pruneHistoryBackupsPending={state.pruneHistoryBackupsMutation.isPending}
        codexHomeDraftValue={state.codexHomeInput}
        onCodexHomeChange={(value) => state.setCodexHomeDraft(value)}
        onSaveConfig={() => state.saveConfigMutation.mutate()}
        gatewayBaseUrl={state.gatewayBaseUrl}
        defaultGatewayBaseUrl={state.defaultGatewayBaseUrl}
        onGatewayBaseUrlChange={(value) => state.setGatewayBaseUrlDraft(value)}
        onUseCurrentGateway={() => state.setGatewayBaseUrlDraft(state.defaultGatewayBaseUrl)}
      />
    </main>
  );
}
