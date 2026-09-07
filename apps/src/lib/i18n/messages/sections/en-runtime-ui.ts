import type { MessageCatalog } from "../types";

export const EN_RUNTIME_UI_MESSAGES: MessageCatalog = {
  "Plugin workspace": "Plugin workspace",
  打开: "Open",
  后刷新: " until refresh",
  本地服务已连接: "Local service connected",
  等待本地服务: "Waiting for local service",
  正在同步状态: "Syncing status",
  状态读取失败: "Failed to read status",
  关闭: "Close",
  "关闭 {label}": "Close {label}",
  "请通过 `codexmanager-web` 打开页面，或在反向代理中同时提供 `/api/runtime` 与 `/api/rpc`。":
    "Open this page through `codexmanager-web`, or expose both `/api/runtime` and `/api/rpc` in the reverse proxy.",
  压缩: "Compact",
  转发: "Forward",
  规范来源: "Canonical source",
  大小拒绝阶段: "Size rejection stage",
  原始路径: "Original path",
  "这里保留旧版和外部部署环境变量覆盖；普通用户优先使用前面结构化设置，高风险项只建议排障时临时修改。":
    "Legacy and external deployment environment overrides are kept here. Regular users should prefer the structured settings above; high-risk items are recommended only for temporary troubleshooting.",
  "会影响运行时配置；修改后请观察请求链路是否稳定。":
    "This affects runtime configuration. After changing it, monitor whether the request path remains stable.",
  删除条目: "Delete entry",
  "全局 User-Agent": "Global User-Agent",
  "留空时使用 Codex 默认 User-Agent：":
    "When blank, use the default Codex User-Agent:",
  "。所有网关上游请求默认使用此值；聚合 API 单独设置时优先。清空后失焦即可恢复默认值。":
    ". This is the default for all gateway upstream requests; a per-Aggregate API value takes priority. Clear the field and leave it to restore the default.",
  "上游 Originator": "Upstream Originator",
  区域驻留要求: "Residency requirement",
  "后，局域网设备可通过当前机器 IP 访问；设置保存后需要重启相关进程才会生效，Web 监听地址会默认跟随这里的模式。":
    "then LAN devices can access this machine by its current IP. After saving, restart the related processes for changes to take effect. The Web bind address follows this mode by default.",
  未知错误: "Unknown error",
  "跟随请求表示使用请求体里的实际 model；请求日志展示的是最终生效模型。":
    "Follow request means using the actual model from the request body; request logs show the final effective model.",
  在图表区域使用鼠标滚轮缩放时间区间:
    "Use the mouse wheel over the chart area to zoom the time range",
  查看服务: "View services",
  内置精选: "Built-in picks",
  "默认使用官方精选插件，适合开箱即用。":
    "Use the official curated plugin catalog by default; suitable for out-of-the-box usage.",
  自定义源: "Custom source",
  "接入你自己的远程 JSON 市场源。":
    "Connect your own remote JSON marketplace source.",
  已安装: "Installed",
  未安装: "Not installed",
  更新: "Update",
};
