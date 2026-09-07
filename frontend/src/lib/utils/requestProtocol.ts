// 统一网关与直连观测的请求类型：服务历史值 ws 和观测值 websocket 表示相同传输，预热单独保留。
export function normalizeRequestType(value: string): "ws" | "wsPrewarm" | "http" | "client" {
  const normalized = String(value || "").trim().toLowerCase();
  // 完成事件没有 HTTP 传输证据，单独呈现来源，不把未知状态或路径标成 HTTP。
  // 单元格和徽章都会调用规范化；规范形式也必须自洽，避免第二次调用把客户端事件改成 HTTP。
  if (normalized === "clientresponse" || normalized === "client") return "client";
  if (normalized === "websocketprewarm" || normalized === "wsprewarm") return "wsPrewarm";
  if (normalized === "ws" || normalized === "websocket" || normalized === "websockethandshake") return "ws";
  return "http";
}
