"use client";

import { useSyncExternalStore } from "react";
import { useI18n } from "@/lib/i18n/provider";
import { formatRemainingDurationFromSeconds } from "@/lib/utils/usage";

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | undefined;
let currentSecond = 0;

// 所有额度共用一个时钟；最后一个订阅卸载时释放定时器，避免账号数量放大计时开销。
function subscribeClock(listener: () => void) {
  listeners.add(listener);
  if (!timer) {
    currentSecond = Math.floor(Date.now() / 1000);
    timer = setInterval(() => {
      currentSecond = Math.floor(Date.now() / 1000);
      listeners.forEach((notify) => notify());
    }, 1000);
  }
  return () => {
    listeners.delete(listener);
    if (!listeners.size) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

// 快照保持稳定，静态导出使用零值，客户端订阅后再显示实际剩余时间。
function readClock() { return currentSecond; }
function readServerClock() { return 0; }

// 只解释上游重置时间；缺失或到期均展示明确状态，不在客户端推算新额度周期。
export function QuotaCountdown({ resetsAt }: { resetsAt?: number | null }) {
  const { t, locale } = useI18n();
  const now = useSyncExternalStore(subscribeClock, readClock, readServerClock);
  const valid = typeof resetsAt === "number" && Number.isFinite(resetsAt) && resetsAt > 0;
  const label = !valid ? t("未提供") : !now ? "—" : resetsAt <= now
    ? t("等待刷新")
    : `${formatRemainingDurationFromSeconds(resetsAt, "days", t("未提供"), locale)} ${t("后重置")}`;
  return <div className="text-[11px] leading-4 tabular-nums text-muted-foreground">{label}</div>;
}
