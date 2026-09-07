"use client";

import { useId, useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Download,
  Github,
  Loader2,
  Package,
  RefreshCw,
  Search,
  Store,
  TerminalSquare,
} from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { WorkPanel } from "@/components/layout/page-workspace";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import {
  CODEX_SKILLS_MARKETPLACE_QUERY_KEY,
  codexSkillsClient,
} from "@/lib/api/codex-skills-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { cn } from "@/lib/utils";
import type {
  CodexSkillMarketplaceInventory,
  CodexSkillMarketplacePlugin,
} from "@/types";

const COLLAPSED_SKILL_COUNT = 3;
const MAX_CONFIRM_SOURCE_LENGTH = 240;

function compactMarketplaceSource(source: string): string {
  if (source.length <= MAX_CONFIRM_SOURCE_LENGTH) return source;
  return `${source.slice(0, MAX_CONFIRM_SOURCE_LENGTH - 1)}…`;
}

function MarketplacePluginCard({
  plugin,
  marketplaceSource,
  expanded,
  installing,
  disabled,
  onToggle,
  onInstall,
}: {
  plugin: CodexSkillMarketplacePlugin;
  marketplaceSource: string;
  expanded: boolean;
  installing: boolean;
  disabled: boolean;
  onToggle: () => void;
  onInstall: () => void;
}) {
  const { t } = useI18n();
  const skillsListId = useId();
  const visibleSkills = expanded
    ? plugin.skills
    : plugin.skills.slice(0, COLLAPSED_SKILL_COUNT);
  const hasMore = plugin.skills.length > COLLAPSED_SKILL_COUNT;

  return (
    <article className="flex min-w-0 flex-col rounded-xl border border-border/60 bg-background/45 p-4 shadow-sm transition-colors hover:bg-background/65">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-sm font-semibold" title={plugin.name}>
              {plugin.name}
            </h3>
            {plugin.version ? (
              <Badge
                variant="outline"
                className="max-w-32 font-mono text-[10px]"
                title={plugin.version}
              >
                <span className="truncate">v{plugin.version}</span>
              </Badge>
            ) : null}
            {plugin.installed ? (
              <Badge
                className={cn(
                  "max-w-full gap-1",
                  plugin.enabled
                    ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-700"
                    : "border-amber-500/25 bg-amber-500/10 text-amber-700",
                )}
              >
                {plugin.enabled ? (
                  <Check className="size-3 shrink-0" />
                ) : (
                  <AlertTriangle className="size-3 shrink-0" />
                )}
                <span className="truncate">
                  {plugin.enabled ? t("已安装") : t("已安装（未启用）")}
                </span>
              </Badge>
            ) : null}
          </div>
          <p
            className="mt-1 truncate font-mono text-[11px] text-muted-foreground"
            title={plugin.pluginId}
          >
            {plugin.pluginId}
          </p>
        </div>
        <Package className="size-5 shrink-0 text-primary/75" />
      </div>

      <p
        className="mt-3 line-clamp-2 min-h-10 break-words text-xs leading-5 text-muted-foreground"
        title={plugin.description || undefined}
      >
        {plugin.description || t("暂无描述")}
      </p>

      <div className="mt-3 flex flex-wrap gap-1.5">
        <Badge
          variant="secondary"
          className="max-w-full"
          title={plugin.marketplaceName}
        >
          <span className="truncate">{plugin.marketplaceName}</span>
        </Badge>
        {plugin.category ? (
          <Badge
            variant="outline"
            className="max-w-full"
            title={plugin.category}
          >
            <span className="truncate">{plugin.category}</span>
          </Badge>
        ) : null}
        {plugin.author ? (
          <Badge variant="outline" className="max-w-full" title={plugin.author}>
            <span className="truncate">{plugin.author}</span>
          </Badge>
        ) : null}
      </div>
      <p className="mt-2 flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
        <span className="shrink-0">{t("来源")}:</span>
        <span className="truncate font-mono" title={marketplaceSource}>
          {marketplaceSource}
        </span>
      </p>

      <div className="mt-4 flex-1 rounded-lg border border-border/50 bg-muted/20 p-3">
        <div className="mb-2 flex items-center justify-between gap-2">
          <span className="text-xs font-medium">
            {t("包含 {count} 个 Codex Skills", { count: plugin.skills.length })}
          </span>
          <Badge variant="secondary" className="font-mono text-[10px]">
            SKILL.md
          </Badge>
        </div>
        <ul id={skillsListId} className="space-y-2">
          {visibleSkills.map((skill) => (
            <li key={skill.name} className="min-w-0">
              <p
                className="truncate font-mono text-[11px] font-medium"
                title={skill.name}
              >
                {skill.name}
              </p>
              {skill.description ? (
                <p
                  className="line-clamp-1 text-[11px] leading-4 text-muted-foreground"
                  title={skill.description}
                >
                  {skill.description}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
        {hasMore ? (
          <Button
            type="button"
            variant="link"
            size="sm"
            className="mt-2 h-auto gap-1 px-0 text-xs"
            onClick={onToggle}
            aria-controls={skillsListId}
            aria-expanded={expanded}
          >
            {expanded
              ? t("收起 Skill 清单")
              : t("查看全部 {count} 个 Skills", {
                  count: plugin.skills.length,
                })}
            <ChevronDown
              className={cn(
                "size-3 transition-transform",
                expanded && "rotate-180",
              )}
            />
          </Button>
        ) : null}
      </div>

      <Button
        type="button"
        className="mt-4 w-full gap-2"
        variant={plugin.installed ? "outline" : "default"}
        disabled={disabled || plugin.installed}
        onClick={onInstall}
      >
        {installing ? (
          <Loader2 className="size-4 animate-spin" />
        ) : plugin.installed ? (
          <Check className="size-4" />
        ) : (
          <Download className="size-4" />
        )}
        {plugin.installed
          ? plugin.enabled
            ? t("已由 Codex 安装")
            : t("已安装但未启用")
          : t("安装完整插件")}
      </Button>
    </article>
  );
}

export function CodexPluginsPanel({
  active,
  enabled,
}: {
  active: boolean;
  enabled: boolean;
}) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [source, setSource] = useState("");
  const [refName, setRefName] = useState("");
  const [search, setSearch] = useState("");
  const [expandedPlugins, setExpandedPlugins] = useState<Set<string>>(
    () => new Set(),
  );
  const [installingPluginId, setInstallingPluginId] = useState<string | null>(
    null,
  );
  const [pendingInstall, setPendingInstall] =
    useState<CodexSkillMarketplacePlugin | null>(null);

  const marketplaceQuery = useQuery({
    queryKey: CODEX_SKILLS_MARKETPLACE_QUERY_KEY,
    queryFn: () => codexSkillsClient.listMarketplace(),
    enabled: active && enabled,
    staleTime: 15_000,
    retry: 1,
  });

  const storeInventory = (inventory: CodexSkillMarketplaceInventory) => {
    queryClient.setQueryData(CODEX_SKILLS_MARKETPLACE_QUERY_KEY, inventory);
  };

  const addMutation = useMutation({
    mutationFn: codexSkillsClient.addMarketplace,
    onSuccess: (inventory) => {
      storeInventory(inventory);
      setSource("");
      setRefName("");
      toast.success(t("Codex Marketplace 已导入"));
    },
    onError: (error) => {
      toast.error(
        `${t("导入 Marketplace 失败")}: ${getAppErrorMessage(error)}`,
      );
    },
  });

  const refreshMutation = useMutation({
    mutationFn: codexSkillsClient.refreshMarketplace,
    onSuccess: (inventory) => {
      storeInventory(inventory);
      toast.success(t("Marketplace 已刷新"));
    },
    onError: (error) => {
      toast.error(
        `${t("刷新 Marketplace 失败")}: ${getAppErrorMessage(error)}`,
      );
    },
  });

  const installMutation = useMutation({
    mutationFn: codexSkillsClient.installMarketplacePlugin,
    onMutate: ({ pluginId }) => {
      setInstallingPluginId(pluginId);
    },
    onSuccess: (inventory) => {
      storeInventory(inventory);
      setPendingInstall(null);
      toast.success(t("插件已安装，新建 Codex 会话后生效"));
    },
    onError: (error) => {
      toast.error(t("安装插件失败"), {
        description: getAppErrorMessage(error),
        duration: 10_000,
        className: "skills-marketplace-install-error-toast",
      });
    },
    onSettled: () => {
      setInstallingPluginId(null);
    },
  });

  const inventory = marketplaceQuery.data;
  const marketplaceSourceByName = useMemo(
    () =>
      new Map(
        (inventory?.marketplaces ?? []).map((marketplace) => [
          marketplace.name,
          marketplace.source || marketplace.name,
        ]),
      ),
    [inventory?.marketplaces],
  );
  const plugins = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return (inventory?.plugins ?? [])
      .filter((plugin) => {
        if (!needle) return true;
        return [
          plugin.name,
          plugin.pluginId,
          plugin.marketplaceName,
          marketplaceSourceByName.get(plugin.marketplaceName) || "",
          plugin.description,
          plugin.author,
          plugin.category,
          ...plugin.skills.flatMap((skill) => [skill.name, skill.description]),
        ].some((value) => value.toLocaleLowerCase().includes(needle));
      })
      .sort(
        (left, right) =>
          Number(right.installed) - Number(left.installed) ||
          left.name.localeCompare(right.name),
      );
  }, [inventory?.plugins, marketplaceSourceByName, search]);
  const installedPluginCount =
    inventory?.plugins.filter((plugin) => plugin.installed).length ?? 0;

  const pendingMarketplaceSource = pendingInstall
    ? marketplaceSourceByName.get(pendingInstall.marketplaceName) ||
      pendingInstall.marketplaceName
    : "";

  const anyMutationPending =
    addMutation.isPending ||
    refreshMutation.isPending ||
    installMutation.isPending;

  const submitSource = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedSource = source.trim();
    if (!normalizedSource) {
      toast.error(t("请输入 GitHub 仓库"));
      return;
    }
    try {
      await addMutation.mutateAsync({
        source: normalizedSource,
        refName: refName.trim() || null,
      });
    } catch {
      // The mutation displays the normalized backend error.
    }
  };

  const togglePlugin = (pluginId: string) => {
    setExpandedPlugins((current) => {
      const next = new Set(current);
      if (next.has(pluginId)) next.delete(pluginId);
      else next.add(pluginId);
      return next;
    });
  };

  return (
    <>
      <WorkPanel className="min-h-0">
        <section
          className="flex min-h-0 flex-col"
          aria-labelledby="codex-plugins-panel-title"
          data-testid="codex-plugins-panel"
        >
        <div className="shrink-0 border-b border-border/60 px-5 py-4 sm:px-6">
          <div className="flex items-start gap-3">
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <Store className="size-5" />
            </div>
            <div className="min-w-0">
              <h2
                id="codex-plugins-panel-title"
                className="text-base font-semibold leading-none"
              >
                {t("Codex 插件市场")}
              </h2>
              <p className="mt-1 text-sm leading-5 text-muted-foreground">
                {t(
                  "通过 Codex 原生 Marketplace 安装完整插件，只展示包含标准 SKILL.md 的插件。",
                )}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {t("插件中的 Skills 会随完整插件一起安装，不能在这里单独安装。")}
              </p>
            </div>
          </div>
        </div>

        <div className="shrink-0 space-y-3 border-b border-border/60 bg-muted/15 px-5 py-4 sm:px-6">
          <form
            className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_9rem_auto]"
            onSubmit={(event) => void submitSource(event)}
          >
            <div className="min-w-0 space-y-1.5">
              <label
                htmlFor="marketplace-source"
                className="block text-xs font-medium text-foreground"
              >
                {t("GitHub Marketplace 仓库")}
              </label>
              <Input
                id="marketplace-source"
                value={source}
                onChange={(event) => setSource(event.target.value)}
                placeholder={t(
                  "GitHub 仓库，例如 openai/role-specific-plugins",
                )}
                autoComplete="off"
                disabled={
                  !enabled || anyMutationPending || !inventory?.cliAvailable
                }
              />
            </div>
            <div className="min-w-0 space-y-1.5">
              <label
                htmlFor="marketplace-ref"
                className="block text-xs font-medium text-foreground"
              >
                {t("分支或标签（可选）")}
              </label>
              <Input
                id="marketplace-ref"
                value={refName}
                onChange={(event) => setRefName(event.target.value)}
                placeholder={t("例如 main")}
                autoComplete="off"
                disabled={
                  !enabled || anyMutationPending || !inventory?.cliAvailable
                }
              />
            </div>
            <Button
              type="submit"
              className="gap-2 sm:self-end"
              disabled={
                !enabled ||
                anyMutationPending ||
                !inventory?.cliAvailable ||
                !source.trim()
              }
            >
              {addMutation.isPending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Github className="size-4" />
              )}
              {t("导入市场")}
            </Button>
          </form>

          {inventory?.marketplaces.length ? (
            <div className="flex min-w-0 items-center gap-1.5 overflow-x-auto pb-1 text-xs text-muted-foreground">
              <span className="shrink-0">{t("已连接市场")}:</span>
              {inventory.marketplaces.map((marketplace) => (
                <Badge
                  key={marketplace.name}
                  variant="secondary"
                  title={marketplace.source || marketplace.name}
                  className="max-w-56 shrink-0"
                >
                  <span className="truncate">{marketplace.name}</span>
                </Badge>
              ))}
            </div>
          ) : null}
        </div>

        <div className="flex shrink-0 flex-col gap-2 border-b border-border/60 px-5 py-3 sm:flex-row sm:items-center sm:px-6">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t("搜索插件、市场或 Skill")}
              aria-label={t("搜索插件、市场或 Skill")}
              className="pl-9"
            />
          </div>
          <div className="flex items-center justify-between gap-2 sm:justify-end">
            <Badge variant="outline" className="shrink-0">
              {t("已安装 {count}", { count: installedPluginCount })}
            </Badge>
            <span className="text-xs text-muted-foreground">
              {t("{count} 个兼容插件", { count: plugins.length })}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="gap-2"
              disabled={
                !enabled || anyMutationPending || !inventory?.cliAvailable
              }
              onClick={() => refreshMutation.mutate({})}
            >
              {refreshMutation.isPending ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <RefreshCw className="size-3.5" />
              )}
              {t("刷新市场")}
            </Button>
          </div>
        </div>

        <ScrollArea
          className="min-h-96"
          style={{ height: "min(64vh, 680px)" }}
          viewportClassName="pr-4"
          scrollbarClassName="skills-marketplace-scrollbar"
          thumbClassName="skills-marketplace-scrollbar-thumb"
          keepScrollbarMounted
          data-testid="skills-marketplace-scroll"
        >
          <div className="p-5 sm:p-6">
            {!enabled ? (
              <Empty className="min-h-64">
                <EmptyMedia variant="icon">
                  <AlertTriangle />
                </EmptyMedia>
                <EmptyHeader>
                  <EmptyTitle>{t("当前无法读取插件市场")}</EmptyTitle>
                  <EmptyDescription>
                    {t("请确认管理 RPC 可用并已连接 codexmanager-service。")}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : marketplaceQuery.isLoading ? (
              <div className="grid gap-4 md:grid-cols-2">
                {Array.from({ length: 4 }).map((_, index) => (
                  <Skeleton key={index} className="h-80 rounded-xl" />
                ))}
              </div>
            ) : marketplaceQuery.error ? (
              <Empty className="min-h-64">
                <EmptyMedia variant="icon">
                  <AlertTriangle />
                </EmptyMedia>
                <EmptyHeader>
                  <EmptyTitle>{t("Marketplace 加载失败")}</EmptyTitle>
                  <EmptyDescription>
                    {getAppErrorMessage(marketplaceQuery.error)}
                  </EmptyDescription>
                </EmptyHeader>
                <Button
                  variant="outline"
                  onClick={() => void marketplaceQuery.refetch()}
                >
                  {t("重试")}
                </Button>
              </Empty>
            ) : !inventory?.cliAvailable ? (
              <Empty className="min-h-64">
                <EmptyMedia variant="icon">
                  <TerminalSquare />
                </EmptyMedia>
                <EmptyHeader>
                  <EmptyTitle>{t("当前 Codex CLI 不支持插件市场")}</EmptyTitle>
                  <EmptyDescription>
                    {t(
                      "请在 codexmanager-service 主机安装或升级支持 plugin 命令的 Codex CLI。",
                    )}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : plugins.length === 0 ? (
              <Empty className="min-h-64">
                <EmptyMedia variant="icon">
                  <Store />
                </EmptyMedia>
                <EmptyHeader>
                  <EmptyTitle>
                    {search
                      ? t("没有匹配的 Marketplace 插件")
                      : t("没有发现兼容的 Codex 插件")}
                  </EmptyTitle>
                  <EmptyDescription>
                    {search
                      ? t("请调整搜索条件。")
                      : t(
                          "导入 GitHub Marketplace；不含 Codex 插件清单或标准 SKILL.md 的插件会被忽略。",
                        )}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : (
              <div className="grid items-start gap-4 md:grid-cols-2">
                {plugins.map((plugin) => (
                  <MarketplacePluginCard
                    key={plugin.pluginId}
                    plugin={plugin}
                    marketplaceSource={
                      marketplaceSourceByName.get(plugin.marketplaceName) ||
                      plugin.marketplaceName
                    }
                    expanded={expandedPlugins.has(plugin.pluginId)}
                    installing={installingPluginId === plugin.pluginId}
                    disabled={!enabled || anyMutationPending}
                    onToggle={() => togglePlugin(plugin.pluginId)}
                    onInstall={() => setPendingInstall(plugin)}
                  />
                ))}
              </div>
            )}

            {inventory?.warnings.map((warning) => (
              <p
                key={warning}
                className="mt-3 flex items-start gap-2 rounded-lg border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-800"
              >
                <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
                <span className="break-words">{warning}</span>
              </p>
            ))}
          </div>
        </ScrollArea>
        </section>
      </WorkPanel>

      <ConfirmDialog
        open={Boolean(pendingInstall)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setPendingInstall(null);
        }}
        title={t("安装完整 Codex 插件")}
        description={t(
          "将安装“{name}”完整插件（市场：{marketplace}；来源：{source}），其中包含 {count} 个 Skills，也可能包含 MCP、Hooks、Apps 或脚本。仅在信任来源时继续。",
          {
            name: pendingInstall?.name || "",
            marketplace: pendingInstall?.marketplaceName || "",
            source: compactMarketplaceSource(pendingMarketplaceSource),
            count: pendingInstall?.skills.length || 0,
          },
        )}
        confirmText={t("确认安装插件")}
        onConfirm={async () => {
          if (!pendingInstall) return false;
          try {
            await installMutation.mutateAsync({
              pluginId: pendingInstall.pluginId,
            });
            return true;
          } catch {
            return false;
          }
        }}
      />
    </>
  );
}
