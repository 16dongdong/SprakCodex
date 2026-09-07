import { invoke, withAddr } from "./transport";
import {
  normalizePluginCatalogResult,
  normalizePluginInstalledList,
  normalizePluginRunLogList,
  normalizePluginTaskList,
} from "./normalize";
import {
  InstalledPluginSummary,
  PluginCatalogEntry,
  PluginCatalogResult,
  PluginRunLogSummary,
  PluginTaskSummary,
} from "../../types";

export const pluginClient = {
  async getCatalog(params?: {
    marketMode?: string;
    sourceUrl?: string;
  }): Promise<PluginCatalogResult> {
    const result = await invoke<unknown>(
      "service_plugin_catalog_list",
      withAddr({
        marketMode: params?.marketMode || null,
        sourceUrl: params?.sourceUrl || null,
      })
    );
    return normalizePluginCatalogResult(result);
  },
  refreshCatalog: () => invoke("service_plugin_catalog_refresh", withAddr()),
  async listInstalled(): Promise<InstalledPluginSummary[]> {
    const result = await invoke<unknown>("service_plugin_list", withAddr());
    return normalizePluginInstalledList(result);
  },
  install: (entry: PluginCatalogEntry) =>
    invoke("service_plugin_install", withAddr({ entry })),
  update: (pluginId: string, sourceUrl?: string) =>
    invoke(
      "service_plugin_update",
      withAddr({
        pluginId,
        sourceUrl: sourceUrl || null,
      })
    ),
  uninstall: (pluginId: string) =>
    invoke("service_plugin_uninstall", withAddr({ pluginId })),
  enable: (pluginId: string) =>
    invoke("service_plugin_enable", withAddr({ pluginId })),
  disable: (pluginId: string) =>
    invoke("service_plugin_disable", withAddr({ pluginId })),
  updateTask: (taskId: string, intervalSeconds: number) =>
    invoke(
      "service_plugin_tasks_update",
      withAddr({ taskId, intervalSeconds })
    ),
  async listTasks(pluginId?: string): Promise<PluginTaskSummary[]> {
    const result = await invoke<unknown>(
      "service_plugin_tasks_list",
      withAddr(pluginId ? { pluginId } : {})
    );
    return normalizePluginTaskList(result);
  },
  runTask: (taskId: string, input?: unknown) =>
    invoke("service_plugin_tasks_run", withAddr({ taskId, input: input ?? null })),
  async listLogs(params?: {
    pluginId?: string;
    taskId?: string;
    limit?: number;
  }): Promise<PluginRunLogSummary[]> {
    const result = await invoke<unknown>(
      "service_plugin_logs_list",
      withAddr({
        pluginId: params?.pluginId || null,
        taskId: params?.taskId || null,
        limit: params?.limit ?? 50,
      })
    );
    return normalizePluginRunLogList(result);
  },
};
