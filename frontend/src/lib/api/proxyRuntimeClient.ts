import { invoke, withAddr } from "./transport";

export interface ProxyNode {
  name: string;
  kind: string;
  server: string;
  port: number;
  chain_entry: string | null;
  sub: string | null;
  [key: string]: unknown;
}

export interface ProxyGroup {
  name: string;
  kind: "select" | "url-test";
  members: string[];
  selected: string | null;
}

export interface Subscription {
  name: string;
  url: string;
  updated_at: number;
  node_count: number;
}

export interface ProxyConfig {
  enabled: boolean;
  active: string | null;
  nodes: ProxyNode[];
  groups: ProxyGroup[];
  mitm: boolean;
  subscriptions: Subscription[];
}

export interface ProxyRuntimeStatus {
  running: boolean;
  mixedPort: number | null;
  active: string | null;
  nodeCount: number;
  groupCount: number;
  subscriptionCount: number;
}

export interface NodeDelay {
  name: string;
  delay: number | null;
  error: string | null;
  country_code: string | null;
}

export const emptyProxyConfig = (): ProxyConfig => ({
  enabled: false,
  active: null,
  nodes: [],
  groups: [],
  mitm: false,
  subscriptions: [],
});

export const proxyRuntimeClient = {
  config: () => invoke<ProxyConfig>("service_proxy_runtime_config", withAddr()),
  status: () => invoke<ProxyRuntimeStatus>("service_proxy_runtime_status", withAddr()),
  save: (config: ProxyConfig) =>
    invoke<ProxyConfig>("service_proxy_runtime_save", withAddr({ config })),
  parse: (text: string) =>
    invoke<ProxyNode[]>("service_proxy_runtime_parse", withAddr({ text })),
  fetch: (url: string) =>
    invoke<ProxyNode[]>("service_proxy_runtime_fetch", withAddr({ url })),
  select: (active: string) =>
    invoke<ProxyRuntimeStatus>("service_proxy_runtime_select", withAddr({ active })),
  selectGroup: (group: string, member: string) =>
    invoke<ProxyRuntimeStatus>(
      "service_proxy_runtime_select_group",
      withAddr({ group, member }),
    ),
  test: () => invoke<NodeDelay[]>("service_proxy_runtime_test", withAddr()),
  egress: () =>
    invoke<Record<string, unknown>>("service_proxy_runtime_egress", withAddr()),
};
