import type { MessageCatalog } from "../types";

export const EN_API_KEYS_MESSAGES: MessageCatalog = {
  "Gateway access": "Gateway access",
  项目: "Project",
  "Token / 金额": "Token / Amount",
  已花费: "Spent",
  不限额: "Unlimited",
  已达上限: "Limit reached",
  管理员视图: "Admin view",
  成员视图: "Member view",
  "请选择平台 Key 归属成员": "Select the member owner for this platform key",
  账号组筛选: "Account group filter",
  账号计划筛选: "Account plan filter",
  账号分组筛选: "Custom account group filter",
  全部分组: "All groups",
  "仅在选中的自定义账号分组内轮转；与账号计划筛选同时设置时，账号必须同时满足两项条件。":
    "Rotate only within the selected custom account group. When a plan filter is also set, accounts must match both filters.",
  "尚未配置账号分组。请先在 OpenAI 账号池中编辑账号并填写分组。":
    "No account groups are configured. Edit an account in the OpenAI account pool and assign a group first.",
  "额度分发开启时，平台 Key 必须归属到一个成员钱包。":
    "When quota distribution is enabled, the platform key must belong to a member wallet.",
  "未开启额度分发时可先不分配，开启后再补齐归属。":
    "When quota distribution is not enabled, you may leave this unassigned and fill in ownership later.",
  "总额度限制 (Token，可选)": "Total quota limit (tokens, optional)",
  不填表示不限制: "Leave blank for no limit",
  K: "K",
  M: "M",
  "达到上限后，这把平台密钥的新请求会被拒绝；已在途请求会按完成后的真实用量继续统计。":
    "After the limit is reached, new requests using this platform key will be rejected. In-flight requests continue to be counted by their final actual usage.",
  按: "By",
  参考估算: "Reference estimate",
};
