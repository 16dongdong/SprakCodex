"use client";

import { useEffect } from "react";
import { useAppStore } from "@/lib/store/useAppStore";

/// 旧根路径只迁移到账号页，不再承载平台仪表盘、钱包或模型摘要。
export default function HomePage() {
  const navigateShellPath = useAppStore((state) => state.navigateShellPath);

  useEffect(() => {
    navigateShellPath("/accounts");
  }, [navigateShellPath]);

  return null;
}
