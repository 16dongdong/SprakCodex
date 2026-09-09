# 会话账号分流

## 运行链路

```text
Codex → DLL 本机接入 → CM → OpenAI 官方服务
```

本机接入负责连接重定向、报文记录和请求身份处理，不修改 Codex 的账号文件、会话文件、项目或模型配置。

## 开关语义

- `sessionRouting.enabled` 默认不存在，等同于关闭。
- 关闭时保留 Codex 原始 `Authorization` 和 `chatgpt-account-id`，仅记录请求。
- 开启后只为新会话创建账号绑定，已有绑定继续保持。
- 开关切换不改变已经开始传输的请求。

## 会话身份

当前已验证的 Codex WebSocket 握手稳定携带 `thread-id` 和 `session-id`。分流按以下顺序选择绑定键：

1. `thread-id`
2. `session-id`
3. 缺少稳定标识时保持原凭据透传

窗口 ID、PID、TCP 连接和项目目录都不作为会话绑定键。

## 原子子账号分配

`session_routing_bindings` 使用会话标识作为主键。首次请求在 SQLite `BEGIN IMMEDIATE` 事务中完成以下步骤：

1. 检查已有绑定。
2. 过滤未参与分流、不可用、令牌过期或额度耗尽的账号。
3. 按活跃绑定数、额度使用率、优先账号和账号排序选择候选。
4. 写入绑定并返回本次不可变凭据快照。

绑定账号后续不可用时进入待迁移状态，下次连接选择可用账号；没有候选时保留等待状态。手动指定目标失效时等待该目标恢复，不擅自改为其他账号。

## 身份字段

分流请求同步替换：

- `Authorization`
- `chatgpt-account-id`

正文、模型、推理等级、工具定义、工具参数、`x-codex-*` 元数据和 WebSocket 帧内容保持原值。

## 日志

请求详情 `formatVersion` 为 `3`，新增 `routing` 对象：

```json
{
  "mode": "passthrough | sessionRouting",
  "sessionId": "SESSION_ID",
  "source": "thread-id | session-id",
  "reason": "routing_disabled | bound_account | missing_stable_session_id"
}
```

请求头入库前统一脱敏，访问令牌不写入日志。

## 会话管理

会话页分页显示本机已识别会话，不读取 Codex 聊天正文。重置绑定仅删除路由记录；下一次连接重新分配。手动换号将记录标记为待切换，已经开始的响应仍使用原身份直至结束。空闲 WebSocket 会检查绑定并重建，不在中途换号或自动重放请求。

账号删除后外键仅清空账号引用，保留会话与待迁移信息。明确 `usage_limit_reached`、`insufficient_quota`、`quota_exceeded` 错误触发账号冷却；没有恢复时间时按五分钟重试间隔处理，普通 429 不等于额度耗尽。

跨账号服务端上下文不保证可复用。旧连接关闭后由客户端建立新请求；上游上下文不兼容错误保留原响应，不自动重复工具执行。
