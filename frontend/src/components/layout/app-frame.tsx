"use client";

import { useEffect } from "react";
import { usePathname } from "next/navigation";
import { Header } from "@/components/layout/header";
import { PageKeepAliveViewport } from "@/components/layout/page-keep-alive-viewport";
import { RouteTransitionOverlay } from "@/components/layout/route-transition-overlay";
import { Sidebar } from "@/components/layout/sidebar";
import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";
import { normalizeRoutePath } from "@/lib/utils/static-routes";

const TRAY_PREVIEW_PATH = "/tray-preview";
const NARROW_VIEWPORT_QUERY = "(max-width: 639px)";

export function isTrayPreviewPath(pathname: string): boolean {
  return normalizeRoutePath(pathname) === TRAY_PREVIEW_PATH;
}

// 管理主窗口的滚动与侧栏生命周期；children 为当前页面，使用真实布局尺寸保证浮层、表格和滚动区域同坐标系。
export function AppFrame({ children }: { children: React.ReactNode }) {
  const { t } = useI18n();
  const pathname = usePathname();
  const isTrayPreview = isTrayPreviewPath(pathname);
  const setSidebarOpen = useAppStore((state) => state.setSidebarOpen);
  const isSidebarOpen = useAppStore((state) => state.isSidebarOpen);

  useEffect(() => {
    document.documentElement.classList.toggle("tray-preview-mode", isTrayPreview);
    document.body.classList.remove("tray-preview-mode");
    return () => {
      document.documentElement.classList.remove("tray-preview-mode");
      document.body.classList.remove("tray-preview-mode");
    };
  }, [isTrayPreview]);

  useEffect(() => {
    const narrowViewport = window.matchMedia(NARROW_VIEWPORT_QUERY);
    const collapseSidebar = () => {
      if (narrowViewport.matches) {
        setSidebarOpen(false);
      }
    };

    collapseSidebar();
    narrowViewport.addEventListener("change", collapseSidebar);
    return () => {
      narrowViewport.removeEventListener("change", collapseSidebar);
    };
  }, [setSidebarOpen]);

  if (isTrayPreview) {
    return <main className="h-screen overflow-hidden bg-transparent">{children}</main>;
  }

  return (
    <div
      className="console-shell flex h-dvh min-w-0 overflow-hidden"
      data-command-center="true"
    >
      {isSidebarOpen ? (
        <button
          type="button"
          className="fixed inset-0 z-50 bg-black/20 sm:hidden"
          aria-label={t("收起侧边栏")}
          onClick={() => setSidebarOpen(false)}
        />
      ) : null}
      <Sidebar />
      <div
        data-slot="app-main-column"
        className="flex min-w-0 flex-1 flex-col overflow-hidden"
      >
        <div
          data-slot="app-main-scale"
          className="flex h-full min-h-0 w-full min-w-0 flex-col"
        >
          <Header />
          <main data-slot="app-content" className="relative min-h-0 min-w-0 flex-1 overflow-y-auto px-4 pb-7 pt-4 lg:px-5 lg:pt-5">
            <RouteTransitionOverlay />
            <PageKeepAliveViewport initialChildren={children} />
          </main>
        </div>
      </div>
    </div>
  );
}
