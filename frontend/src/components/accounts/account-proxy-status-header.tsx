"use client";

import { cn } from "@/lib/utils";

export interface AccountProxyStatusHeaderProps {
	status: string | null | undefined;
	latencyMs: number | null | undefined;
	lastTestedAt: number | null | undefined;
	t: (key: string) => string;
}

export function AccountProxyStatusHeader({
	status,
	latencyMs,
	lastTestedAt,
	t,
}: AccountProxyStatusHeaderProps) {
	const normalizedStatus = String(status || "not_configured");

	const statusText = (() => {
		switch (normalizedStatus) {
			case "ok":
				return t("健康");
			case "checking":
				return t("检查中");
			case "failed":
				return t("失败");
			case "runtime_error":
				return t("运行错误");
			case "invalid_url":
				return t("无效URL");
			case "unchecked":
				return t("未检查");
			case "not_configured":
			default:
				return t("未配置");
		}
	})();

	const statusColorClass = (() => {
		switch (normalizedStatus) {
			case "ok":
				return "text-green-600 dark:text-green-400";
			case "checking":
				return "text-yellow-600 dark:text-yellow-400";
			case "unchecked":
				return "text-orange-600 dark:text-orange-400";
			case "failed":
			case "runtime_error":
			case "invalid_url":
				return "text-red-600 dark:text-red-400";
			case "not_configured":
			default:
				return "text-muted-foreground";
		}
	})();

	const lastCheckText =
		lastTestedAt != null
			? new Date(lastTestedAt * 1000).toLocaleString()
			: t("从未检查");

	return (
		<div className="flex flex-wrap items-start gap-x-12 gap-y-3 border-b border-border/50 pb-4 mb-4">
			<div>
				<div className="text-[10px] font-medium text-muted-foreground/80 uppercase tracking-wider">
					{t("测试状态")}
				</div>
				<div className={cn("mt-0.5 text-xs font-normal", statusColorClass)}>
					{statusText}
					{normalizedStatus === "ok" && latencyMs != null && (
						<span className="text-muted-foreground font-normal">
							{" • "}{latencyMs} {t("ms")}
						</span>
					)}
				</div>
			</div>
			<div>
				<div className="text-[10px] font-medium text-muted-foreground/80 uppercase tracking-wider">
					{t("最近检查")}
				</div>
				<div className="mt-0.5 text-xs font-normal text-foreground">
					{lastCheckText}
				</div>
			</div>
		</div>
	);
}
