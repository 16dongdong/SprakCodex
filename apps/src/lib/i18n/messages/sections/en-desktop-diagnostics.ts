import type { MessageCatalog } from "../types";

export const EN_DESKTOP_DIAGNOSTICS_MESSAGES: MessageCatalog = {
  诊断设置已更新: "Diagnostic settings updated",
  更新诊断设置失败: "Failed to update diagnostic settings",
  打开日志目录失败: "Failed to open the log directory",
  "当前 Web / Docker 版不支持桌面诊断设置":
    "Desktop diagnostic settings are unavailable in the Web / Docker edition",
  桌面诊断: "Desktop diagnostics",
  "记录桌面启动异常，并控制本地运行日志":
    "Capture desktop startup failures and control local runtime logs",
  诊断信息读取失败: "Failed to load diagnostic information",
  "Debug 模式": "Debug mode",
  "记录更详细的桌面启动与运行信息，设置在重启后继续生效":
    "Record more detailed desktop startup and runtime information; the setting persists after restart",
  "本次启动已通过 --debug 临时启用 Debug 模式和文件日志":
    "Debug mode and file logging were temporarily enabled for this launch with --debug",
  本地文件日志: "Local file logging",
  "关闭后停止写入桌面运行日志；请求日志、Token 与费用统计不受影响":
    "When disabled, desktop runtime logs stop being written; request logs, token usage, and cost statistics are unaffected",
  "运行日志单文件最多 512 KB，达到上限后自动覆盖":
    "The runtime log is limited to 512 KB and is overwritten when it reaches the limit",
  最近一次启动失败: "Most recent startup failure",
};
