// 统一网关与直连观测的请求类型：服务历史值 ws 和观测值 websocket 表示相同传输，预热单独保留。
export function normalizeRequestType(value: string): "ws" | "wsPrewarm" | "http" {
  const normalized = String(value || "").trim().toLowerCase();
  if (normalized === "websocketprewarm" || normalized === "wsprewarm") return "wsPrewarm";
  if (normalized === "ws" || normalized === "websocket" || normalized === "websockethandshake") return "ws";
  return "http";
}
