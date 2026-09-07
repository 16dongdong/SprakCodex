"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { accountClient } from "@/lib/api/account-client";
import {
  buildApiKeyListQueryKey,
  buildManagedModelSelectorQueryKey,
} from "@/lib/api/account-query-keys";
import {
  managedModelsV2Client,
  managedModelV2ToModelInfo,
} from "@/lib/api/managed-models-v2";
import { CODEX_PROFILE_CANDIDATES_QUERY_KEY } from "@/lib/api/codex-profile-client";
import {
  buildStartupSnapshotQueryKey,
  STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
} from "@/lib/api/startup-snapshot";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useLocalDayRange } from "@/hooks/useLocalDayRange";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import { StartupSnapshot } from "@/types";

type ApiKeyPayload = Parameters<typeof accountClient.createApiKey>[0];

/**
 * 函数 `useApiKeys`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * 无
 *
 * # 返回
 * 返回函数执行结果
 */
export function useApiKeys() {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const localDayRange = useLocalDayRange();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const isPageActive = useDesktopPageActive("/apikeys/");
  const areApiKeyQueriesEnabled = useDeferredDesktopActivation(
    isServiceReady && isPageActive,
  );
  const startupSnapshot = queryClient.getQueryData<StartupSnapshot>(
    buildStartupSnapshotQueryKey(
      serviceStatus.addr,
      STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
      localDayRange.dayStartTs,
      localDayRange.dayEndTs,
    )
  );
  const startupApiKeys = startupSnapshot?.apiKeys || [];
  const startupApiModels = startupSnapshot?.apiModels;
  const hasStartupApiKeySnapshot = startupApiKeys.length > 0;
  const hasStartupApiModelSnapshot = (startupApiModels?.models?.length || 0) > 0;
  const apiKeyListQueryKey = buildApiKeyListQueryKey(serviceStatus.addr);
  const managedModelSelectorQueryKey = buildManagedModelSelectorQueryKey(
    serviceStatus.addr,
  );

  /**
   * 函数 `ensureServiceReady`
   *
   * 作者: gaohongshun
   *
   * 时间: 2026-04-02
   *
   * # 参数
   * - actionLabel: 参数 actionLabel
   *
   * # 返回
   * 返回函数执行结果
   */
  const ensureServiceReady = (actionLabel: string): boolean => {
    if (isServiceReady) {
      return true;
    }
    toast.info(`${t("服务未连接，暂时无法")} ${t(actionLabel)}`);
    return false;
  };

  const apiKeysQuery = useQuery({
    queryKey: apiKeyListQueryKey,
    queryFn: () => accountClient.listApiKeys(serviceStatus.addr),
    enabled: areApiKeyQueriesEnabled,
    retry: 1,
    placeholderData: () =>
      startupApiKeys.length > 0 ? startupApiKeys : undefined,
  });

  const modelsQuery = useQuery({
    queryKey: managedModelSelectorQueryKey,
    queryFn: async () => {
      const result = await managedModelsV2Client.list(false, serviceStatus.addr);
      return { models: result.items.map(managedModelV2ToModelInfo) };
    },
    enabled: areApiKeyQueriesEnabled,
    retry: 1,
    placeholderData: () =>
      startupApiModels?.models?.length ? startupApiModels : undefined,
  });

  /**
   * 函数 `invalidateAll`
   *
   * 作者: gaohongshun
   *
   * 时间: 2026-04-02
   *
   * # 参数
   * 无
   *
   * # 返回
   * 返回函数执行结果
   */
  const invalidateAll = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["apikeys"] }),
      queryClient.invalidateQueries({ queryKey: ["managed-models-v2"] }),
      queryClient.invalidateQueries({ queryKey: ["apikey-usage-stats"] }),
      queryClient.invalidateQueries({ queryKey: ["apikey-usage-overview"] }),
      queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
      queryClient.invalidateQueries({ queryKey: CODEX_PROFILE_CANDIDATES_QUERY_KEY }),
    ]);
  };

  const createMutation = useMutation({
    mutationFn: (params: ApiKeyPayload) =>
      accountClient.createApiKey(params, serviceStatus.addr),
    onSuccess: async () => {
      await invalidateAll();
      toast.success(t("密钥已创建"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("创建失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) =>
      accountClient.deleteApiKey(id, serviceStatus.addr),
    onSuccess: async () => {
      await invalidateAll();
      toast.success(t("密钥已删除"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("删除失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, params }: { id: string; params: ApiKeyPayload }) =>
      accountClient.updateApiKey(id, params, serviceStatus.addr),
    onSuccess: async (_result, variables) => {
      queryClient.setQueryData(apiKeyListQueryKey, (current: unknown) =>
        Array.isArray(current)
          ? current.map((item) =>
              item && typeof item === "object" && "id" in item && item.id === variables.id
                ? {
                    ...item,
                    rotationStrategy:
                      variables.params.rotationStrategy ?? item.rotationStrategy,
                    aggregateApiId:
                      variables.params.aggregateApiId ?? item.aggregateApiId,
                    accountPlanFilter:
                      "accountPlanFilter" in variables.params
                        ? variables.params.accountPlanFilter ?? null
                        : item.accountPlanFilter,
                    accountGroupFilter:
                      "accountGroupFilter" in variables.params
                        ? variables.params.accountGroupFilter ?? null
                        : item.accountGroupFilter,
                    quotaLimitTokens:
                      "quotaLimitTokens" in variables.params
                        ? variables.params.quotaLimitTokens ?? null
                        : item.quotaLimitTokens,
                  }
                : item,
            )
          : current,
      );
      await invalidateAll();
      toast.success(t("密钥配置已更新"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("更新失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const toggleStatusMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enabled
        ? accountClient.enableApiKey(id, serviceStatus.addr)
        : accountClient.disableApiKey(id, serviceStatus.addr),
    onSuccess: async () => {
      await invalidateAll();
      toast.success(t("状态已更新"));
    },
    onError: (error: unknown) => {
      toast.error(`${t("更新状态失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const readSecretMutation = useMutation({
    mutationFn: (id: string) =>
      accountClient.readApiKeySecret(id, serviceStatus.addr),
    onError: (error: unknown) => {
      toast.error(`${t("读取密钥失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  return {
    apiKeys: apiKeysQuery.data || [],
    modelCatalog: modelsQuery.data || { models: [] },
    models: modelsQuery.data?.models || [],
    isLoading:
      isServiceReady &&
      !hasStartupApiKeySnapshot &&
      (!areApiKeyQueriesEnabled || apiKeysQuery.isLoading),
    isModelsLoading:
      isServiceReady &&
      !hasStartupApiModelSnapshot &&
      (!areApiKeyQueriesEnabled || modelsQuery.isLoading),
    isServiceReady,
    createApiKey: async (params: ApiKeyPayload) => {
      if (!ensureServiceReady("创建密钥")) return;
      await createMutation.mutateAsync(params);
    },
    deleteApiKey: (id: string) => {
      if (!ensureServiceReady("删除密钥")) return;
      deleteMutation.mutate(id);
    },
    updateApiKey: async (id: string, params: ApiKeyPayload) => {
      if (!ensureServiceReady("更新密钥")) return;
      await updateMutation.mutateAsync({ id, params });
    },
    toggleApiKeyStatus: ({ id, enabled }: { id: string; enabled: boolean }) => {
      if (!ensureServiceReady(enabled ? "启用密钥" : "禁用密钥")) return;
      toggleStatusMutation.mutate({ id, enabled });
    },
    refreshModels: () => {
      if (!ensureServiceReady("刷新模型")) return;
      void modelsQuery.refetch().then((result) => {
        if (result.error) {
          toast.error(`${t("刷新模型失败")}: ${getAppErrorMessage(result.error)}`);
          return;
        }
        toast.success(t("模型列表已刷新"));
      });
    },
    readApiKeySecret: async (id: string) => {
      if (!ensureServiceReady("读取密钥")) return "";
      return await readSecretMutation.mutateAsync(id);
    },
    isToggling: toggleStatusMutation.isPending,
    isRefreshingModels: modelsQuery.isRefetching,
    isReadingSecret: readSecretMutation.isPending,
  };
}
