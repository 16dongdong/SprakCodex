"use client";

import { useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Check,
  Github,
  Loader2,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  CODEX_SKILLS_REPOSITORIES_QUERY_KEY,
  codexSkillsClient,
} from "@/lib/api/codex-skills-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import type {
  CodexSkillRepositoryCatalog,
  CodexSkillRepositorySummary,
} from "@/types";

function repositoryLabel(repository: CodexSkillRepositorySummary): string {
  return (
    repository.name ||
    [repository.owner, repository.repository].filter(Boolean).join("/") ||
    repository.id
  );
}

function formatScannedAt(value: number): string {
  const date = new Date(value * 1_000);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString();
}

export function SkillRepositoriesDialog({
  open,
  onOpenChange,
  enabled,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  enabled: boolean;
}) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [source, setSource] = useState("");
  const [refName, setRefName] = useState("");
  const [pendingDelete, setPendingDelete] =
    useState<CodexSkillRepositorySummary | null>(null);
  const [refreshingRepositoryId, setRefreshingRepositoryId] = useState<
    string | null
  >(null);

  const repositoriesQuery = useQuery({
    queryKey: CODEX_SKILLS_REPOSITORIES_QUERY_KEY,
    queryFn: () => codexSkillsClient.listRepositories(),
    enabled: open && enabled,
    staleTime: 15_000,
    retry: 1,
  });

  const storeCatalog = (catalog: CodexSkillRepositoryCatalog) => {
    queryClient.setQueryData(CODEX_SKILLS_REPOSITORIES_QUERY_KEY, catalog);
  };

  const addMutation = useMutation({
    mutationFn: codexSkillsClient.addRepository,
    onSuccess: (catalog) => {
      storeCatalog(catalog);
      setSource("");
      setRefName("");
      toast.success(t("技能仓库已添加"));
    },
    onError: (error) => {
      toast.error(`${t("添加仓库失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const refreshMutation = useMutation({
    mutationFn: codexSkillsClient.refreshRepository,
    onSuccess: (catalog) => {
      storeCatalog(catalog);
      toast.success(t("技能仓库已刷新"));
    },
    onError: (error) => {
      toast.error(`${t("刷新仓库失败")}: ${getAppErrorMessage(error)}`);
    },
    onSettled: () => setRefreshingRepositoryId(null),
  });

  const deleteMutation = useMutation({
    mutationFn: codexSkillsClient.deleteRepository,
    onSuccess: (catalog) => {
      storeCatalog(catalog);
      setPendingDelete(null);
      toast.success(t("技能仓库已删除，已安装的 Skills 不受影响"));
    },
    onError: (error) => {
      toast.error(`${t("删除仓库失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const busy =
    addMutation.isPending ||
    refreshMutation.isPending ||
    deleteMutation.isPending;

  const submitRepository = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!enabled || !source.trim()) return;
    try {
      await addMutation.mutateAsync({
        source: source.trim(),
        refName: refName.trim() || null,
      });
    } catch {
      // The mutation displays the normalized error.
    }
  };

  const refreshRepository = async (repositoryId?: string) => {
    setRefreshingRepositoryId(repositoryId || "all");
    try {
      await refreshMutation.mutateAsync({ repositoryId: repositoryId || null });
    } catch {
      // The mutation displays the normalized error.
    }
  };

  const confirmDeleteRepository = async (repositoryId: string) => {
    try {
      await deleteMutation.mutateAsync({ repositoryId });
    } catch {
      // The mutation displays the normalized error.
    }
  };

  const repositories = repositoriesQuery.data?.repositories ?? [];

  return (
    <Dialog open={open} onOpenChange={busy ? undefined : onOpenChange}>
      <DialogContent className="max-h-[min(90dvh,900px)] overflow-y-auto sm:!max-w-[min(92vw,980px)]">
        <DialogHeader>
          <DialogTitle>{t("管理技能仓库")}</DialogTitle>
          <DialogDescription>
            {t("添加公共 GitHub 仓库并同步其中的 SKILL.md。删除仓库不会删除已安装的 Skills。")}
          </DialogDescription>
        </DialogHeader>

        <form
          className="grid gap-3 rounded-lg border border-border/60 bg-muted/20 p-4 sm:grid-cols-[minmax(0,1fr)_11rem_auto]"
          onSubmit={(event) => void submitRepository(event)}
        >
          <div className="space-y-1.5">
            <label htmlFor="skill-repository-source" className="text-xs font-medium">
              {t("GitHub 仓库 URL")}
            </label>
            <Input
              id="skill-repository-source"
              value={source}
              onChange={(event) => setSource(event.target.value)}
              placeholder="https://github.com/owner/repository"
              autoComplete="off"
              disabled={!enabled || busy}
            />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="skill-repository-ref" className="text-xs font-medium">
              {t("分支或标签")}
            </label>
            <Input
              id="skill-repository-ref"
              value={refName}
              onChange={(event) => setRefName(event.target.value)}
              placeholder={t("默认分支")}
              autoComplete="off"
              disabled={!enabled || busy}
            />
          </div>
          <Button
            type="submit"
            className="self-end gap-2"
            disabled={!enabled || busy || !source.trim()}
          >
            {addMutation.isPending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Plus className="size-4" />
            )}
            {t("添加仓库")}
          </Button>
        </form>

        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium">{t("已连接仓库")}</p>
            <p className="text-xs text-muted-foreground">
              {t("共 {count} 个仓库", { count: repositories.length })}
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="gap-2"
            disabled={!enabled || busy}
            onClick={() => void refreshRepository()}
          >
            <RefreshCw
              className={
                refreshingRepositoryId === "all"
                  ? "size-3.5 animate-spin"
                  : "size-3.5"
              }
            />
            {t("全部刷新")}
          </Button>
        </div>

        {!enabled ? (
          <div className="rounded-lg border border-amber-500/25 bg-amber-500/10 p-4 text-sm text-amber-800">
            {t("请先连接 codexmanager-service。")}
          </div>
        ) : repositoriesQuery.isLoading ? (
          <div className="flex min-h-32 items-center justify-center">
            <Loader2 className="size-5 animate-spin text-muted-foreground" />
          </div>
        ) : repositoriesQuery.error ? (
          <div className="flex items-start gap-2 rounded-lg border border-destructive/25 bg-destructive/10 p-4 text-sm text-destructive">
            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
            <span>{getAppErrorMessage(repositoriesQuery.error)}</span>
          </div>
        ) : repositories.length === 0 ? (
          <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
            {t("尚未添加技能仓库")}
          </div>
        ) : (
          <div className="divide-y divide-border/60 rounded-lg border border-border/60">
            {repositories.map((repository) => {
              const label = repositoryLabel(repository);
              const confirmingDelete = pendingDelete?.id === repository.id;
              return (
                <div key={repository.id} className="space-y-3 p-4">
                  <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                    <div className="min-w-0 space-y-1.5">
                      <div className="flex flex-wrap items-center gap-2">
                        <Github className="size-4 text-muted-foreground" />
                        <span className="truncate text-sm font-semibold" title={label}>
                          {label}
                        </span>
                        {repository.builtin ? (
                          <Badge variant="secondary" className="gap-1">
                            <ShieldCheck className="size-3" />
                            {t("内置")}
                          </Badge>
                        ) : null}
                        {repository.lastError ? (
                          <Badge className="border-amber-500/25 bg-amber-500/10 text-amber-700">
                            {t("同步失败")}
                          </Badge>
                        ) : repository.lastScannedAt ? (
                          <Badge className="border-emerald-500/25 bg-emerald-500/10 text-emerald-700">
                            <Check className="mr-1 size-3" />
                            {t("已同步")}
                          </Badge>
                        ) : (
                          <Badge variant="outline">{t("等待同步")}</Badge>
                        )}
                      </div>
                      <p
                        className="truncate font-mono text-[11px] text-muted-foreground"
                        title={repository.sourceUrl}
                      >
                        {repository.sourceUrl || repository.id}
                        {repository.refName ? ` · ${repository.refName}` : ""}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {t("发现 {count} 个 Skills", {
                          count: repository.skillCount,
                        })}
                        {repository.lastScannedAt
                          ? ` · ${t("最近同步")} ${formatScannedAt(repository.lastScannedAt)}`
                          : ""}
                      </p>
                      {repository.lastError ? (
                        <p className="break-words text-xs text-amber-700">
                          {repository.lastError}
                        </p>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 flex-wrap gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="gap-1.5"
                        disabled={busy}
                        onClick={() => void refreshRepository(repository.id)}
                      >
                        <RefreshCw
                          className={
                            refreshingRepositoryId === repository.id
                              ? "size-3.5 animate-spin"
                              : "size-3.5"
                          }
                        />
                        {t("刷新")}
                      </Button>
                      {!repository.builtin ? (
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="gap-1.5 text-destructive hover:text-destructive"
                          disabled={busy}
                          onClick={() => setPendingDelete(repository)}
                        >
                          <Trash2 className="size-3.5" />
                          {t("删除")}
                        </Button>
                      ) : null}
                    </div>
                  </div>

                  {confirmingDelete ? (
                    <div className="flex flex-col gap-3 rounded-md border border-destructive/25 bg-destructive/5 p-3 text-xs sm:flex-row sm:items-center sm:justify-between">
                      <span>
                        {t("确定删除仓库“{name}”？已安装的 Skills 会保留。", {
                          name: label,
                        })}
                      </span>
                      <div className="flex shrink-0 gap-2">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={deleteMutation.isPending}
                          onClick={() => setPendingDelete(null)}
                        >
                          {t("取消")}
                        </Button>
                        <Button
                          type="button"
                          variant="destructive"
                          size="sm"
                          disabled={deleteMutation.isPending}
                          onClick={() => void confirmDeleteRepository(repository.id)}
                        >
                          {deleteMutation.isPending ? (
                            <Loader2 className="size-3.5 animate-spin" />
                          ) : null}
                          {t("确认删除")}
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("关闭")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
