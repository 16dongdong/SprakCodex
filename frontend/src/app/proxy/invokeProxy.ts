import { observationClient } from "@/lib/api/observationClient";
import {
  proxyRuntimeClient,
  type ProxyConfig,
} from "@/lib/api/proxyRuntimeClient";

let lastEnabled = false;

// 保留迁移页面原有命令名，让源文件只替换传输入口；所有调用最终进入本项目 typed client。
export async function invokeProxy<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_config": {
      const proxy = await proxyRuntimeClient.config();
      lastEnabled = proxy.enabled;
      return { targets: { targets: [], target_pids: [], exclude_pids: [] }, proxy } as T;
    }
    case "save_proxy_config": {
      const proxy = (args?.proxy ?? {}) as ProxyConfig;
      const restart = proxy.enabled !== lastEnabled;
      await proxyRuntimeClient.save(proxy);
      lastEnabled = proxy.enabled;
      if (restart) {
        const observation = await observationClient.status();
        if (observation.running) {
          await observationClient.stop();
          await observationClient.start();
        }
      }
      return undefined as T;
    }
    case "test_nodes":
      return (await proxyRuntimeClient.test()) as T;
    case "test_node":
      return (await proxyRuntimeClient.testNode(String(args?.name ?? ""))) as T;
    case "select_group_member":
      return (await proxyRuntimeClient.selectGroup(String(args?.group ?? ""), String(args?.member ?? ""))) as T;
    case "fetch_subscription":
      return (await proxyRuntimeClient.fetch(String(args?.url ?? ""))) as T;
    case "parse_clash_text":
      return (await proxyRuntimeClient.parse(String(args?.text ?? ""))) as T;
    case "test_egress":
      return (await proxyRuntimeClient.egress()) as T;
    default:
      throw new Error(`未知代理命令：${command}`);
  }
}
