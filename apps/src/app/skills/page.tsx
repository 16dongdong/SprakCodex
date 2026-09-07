"use client";

import {
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
} from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FolderInput, Loader2, Package, WandSparkles } from "lucide-react";
import { toast } from "sonner";
import { PageHeader, PageWorkspace } from "@/components/layout/page-workspace";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import {
  CODEX_SKILLS_QUERY_KEY,
  CODEX_SKILLS_REGISTRY_QUERY_KEY,
  CODEX_SKILLS_REPOSITORIES_QUERY_KEY,
  MAX_CODEX_SKILL_ZIP_BYTES,
  codexSkillsClient,
} from "@/lib/api/codex-skills-client";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import type { CodexSkillsInventory } from "@/types";
import { CodexPluginsPanel } from "./marketplace-dialog";
import { SkillsCatalogPanel } from "./skills-catalog-panel";

const BASE64_CHUNK_BYTES = 32 * 1024;
const PAGE_TABS = ["skills", "plugins"] as const;
type PageTab = (typeof PAGE_TABS)[number];

function encodeArrayBufferBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += BASE64_CHUNK_BYTES) {
    binary += String.fromCharCode(
      ...bytes.subarray(offset, offset + BASE64_CHUNK_BYTES),
    );
  }
  return btoa(binary);
}

function formatMiB(bytes: number): string {
  return `${Math.round(bytes / (1024 * 1024))} MiB`;
}

export default function SkillsPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const serviceConnected = useAppStore((state) => state.serviceStatus.connected);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isPageActive = useDesktopPageActive("/skills/");
  const isReady = useDeferredDesktopActivation(
    isPageActive && serviceConnected && canAccessManagementRpc,
  );
  usePageTransitionReady(
    "/skills/",
    !serviceConnected || !canAccessManagementRpc || isReady,
  );

  const [activeTab, setActiveTab] = useState<PageTab>("skills");
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [sourcePath, setSourcePath] = useState("");
  const [preparingZip, setPreparingZip] = useState(false);

  const syncInventory = (inventory: CodexSkillsInventory) => {
    queryClient.setQueryData(CODEX_SKILLS_QUERY_KEY, inventory);
    void queryClient.invalidateQueries({
      queryKey: CODEX_SKILLS_REPOSITORIES_QUERY_KEY,
    });
    void queryClient.invalidateQueries({
      queryKey: CODEX_SKILLS_REGISTRY_QUERY_KEY,
    });
  };

  const installMutation = useMutation({
    mutationFn: codexSkillsClient.installZip,
    onSuccess: (inventory) => {
      syncInventory(inventory);
      toast.success(t("Skill ZIP 已安装"));
    },
    onError: (error) => {
      toast.error(`${t("安装失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const importMutation = useMutation({
    mutationFn: codexSkillsClient.importDirectory,
    onSuccess: (inventory) => {
      syncInventory(inventory);
      setImportDialogOpen(false);
      setSourcePath("");
      toast.success(t("Skill 目录已导入"));
    },
    onError: (error) => {
      toast.error(`${t("导入失败")}: ${getAppErrorMessage(error)}`);
    },
  });

  const manualInstallPending =
    preparingZip || installMutation.isPending || importMutation.isPending;

  const chooseZip = () => {
    const input = fileInputRef.current;
    if (!input || manualInstallPending) return;
    input.value = "";
    input.click();
  };

  const handleZipChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) {
      input.value = "";
      return;
    }
    setPreparingZip(true);
    try {
      if (!file.name.toLocaleLowerCase().endsWith(".zip")) {
        throw new Error(t("请选择 ZIP 文件"));
      }
      if (file.size > MAX_CODEX_SKILL_ZIP_BYTES) {
        throw new Error(
          t("ZIP 文件不能超过 {size}", {
            size: formatMiB(MAX_CODEX_SKILL_ZIP_BYTES),
          }),
        );
      }
      const archiveBase64 = encodeArrayBufferBase64(await file.arrayBuffer());
      try {
        await installMutation.mutateAsync({
          fileName: file.name,
          archiveBase64,
        });
      } catch {
        // The mutation displays the normalized backend error.
      }
    } catch (error) {
      toast.error(getAppErrorMessage(error));
    } finally {
      setPreparingZip(false);
      input.value = "";
    }
  };

  const submitImport = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedPath = sourcePath.trim();
    if (!normalizedPath) {
      toast.error(t("请输入服务主机上的绝对路径"));
      return;
    }
    try {
      await importMutation.mutateAsync({ sourcePath: normalizedPath });
    } catch {
      // The mutation displays the normalized error.
    }
  };

  return (
    <PageWorkspace>
      <input
        ref={fileInputRef}
        type="file"
        accept=".zip,application/zip"
        className="hidden"
        onChange={(event) => void handleZipChange(event)}
      />

      <PageHeader
        eyebrow="CODEX"
        title={t("Skills 与插件")}
        description={t("安装独立 Skills，或管理 Codex 原生插件。")}
        meta={
          <Badge variant="outline">
            {activeTab === "skills" ? t("Skills 安装") : t("Codex 插件安装")}
          </Badge>
        }
      />

      <Tabs
        value={activeTab}
        onValueChange={(value) => {
          if (value && PAGE_TABS.includes(value as PageTab)) {
            setActiveTab(value as PageTab);
          }
        }}
        className="w-full gap-4"
      >
        <TabsList className="glass-card mission-panel grid h-11 w-full grid-cols-2 rounded-lg p-1 sm:w-[430px]">
          <TabsTrigger value="skills" className="gap-2 px-5">
            <WandSparkles className="size-4" />
            {t("Skills 安装")}
          </TabsTrigger>
          <TabsTrigger value="plugins" className="gap-2 px-5">
            <Package className="size-4" />
            {t("Codex 插件安装")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="skills">
          <SkillsCatalogPanel
            enabled={isReady}
            manualInstallPending={manualInstallPending}
            onInstallZip={chooseZip}
            onImportDirectory={() => setImportDialogOpen(true)}
          />
        </TabsContent>

        <TabsContent value="plugins" keepMounted>
          <CodexPluginsPanel active={activeTab === "plugins"} enabled={isReady} />
        </TabsContent>
      </Tabs>

      <Dialog open={importDialogOpen} onOpenChange={setImportDialogOpen}>
        <DialogContent showCloseButton={!importMutation.isPending}>
          <form onSubmit={(event) => void submitImport(event)}>
            <DialogHeader>
              <DialogTitle>{t("导入已有 Skill 目录")}</DialogTitle>
              <DialogDescription>
                {t("输入 codexmanager-service 主机上的绝对路径。目录根部必须包含 SKILL.md。")}
              </DialogDescription>
            </DialogHeader>
            <div className="py-5">
              <label
                htmlFor="skill-source-path"
                className="mb-2 block text-sm font-medium"
              >
                {t("服务主机绝对路径")}
              </label>
              <Input
                id="skill-source-path"
                value={sourcePath}
                onChange={(event) => setSourcePath(event.target.value)}
                placeholder={t("例如 /opt/codex-skills/my-skill")}
                autoComplete="off"
                autoFocus
                disabled={importMutation.isPending}
              />
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={importMutation.isPending}
                onClick={() => setImportDialogOpen(false)}
              >
                {t("取消")}
              </Button>
              <Button
                type="submit"
                disabled={importMutation.isPending || !sourcePath.trim()}
              >
                {importMutation.isPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <FolderInput className="size-4" />
                )}
                {t("导入")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </PageWorkspace>
  );
}
