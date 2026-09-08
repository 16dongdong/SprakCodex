"use client";

import { useAppStore } from "@/lib/store/useAppStore";
import { useI18n } from "@/lib/i18n/provider";
import { getTopLevelRouteLabel } from "@/lib/app-shell/top-level-routes";

// 顶栏只承担页面定位；接入控制留在账号页，语言与声明统一归入设置，避免重复入口。
export function Header() {
  const currentShellPath = useAppStore((state) => state.currentShellPath);
  const { t } = useI18n();
  return (
    <header className="sticky top-0 z-30 flex min-h-[64px] shrink-0 items-center glass-header px-4 lg:px-5">
      <div className="header-title-group min-w-0 flex-1">
        <h1 className="header-page-title min-w-0 truncate text-lg font-semibold tracking-tight text-foreground sm:text-[21px] xl:text-2xl">
          {t(getTopLevelRouteLabel(currentShellPath))}
        </h1>
      </div>
    </header>
  );
}
