import { invoke, withAddr } from "./transport";

export interface ObservationStatus {
  running: boolean;
  proxyUrl: string | null;
  certificatePath: string | null;
  writtenRequests: number;
  storageErrors: number;
}

export const observationQueryKey = ["directObservation", "status"] as const;

// 后端返回公开状态而非认证材料；同一入口复用桌面与 Web 认证传输，错误交给调用方展示。
async function query(action: "status" | "start" | "stop"): Promise<ObservationStatus> {
  return invoke<ObservationStatus>(`service_observation_${action}`, withAddr());
}

export const observationClient = {
  // 状态属于当前服务进程，不把上次运行状态写入浏览器持久缓存。
  status: () => query("status"),
  // 显式启用临时证书与监听；失败保留原来的关闭状态。
  start: () => query("start"),
  // 等待后端清理后刷新状态，不在客户端先行伪造成功。
  stop: () => query("stop"),
};
