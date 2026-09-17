export type { ProxyConfig, ProxyGroup, ProxyNode, Subscription } from "@/lib/api/proxyRuntimeClient";
export interface TargetsConfig { targets: string[]; target_pids: number[]; exclude_pids: number[]; }
export interface ConfigView { targets: TargetsConfig; proxy: import("@/lib/api/proxyRuntimeClient").ProxyConfig; }
