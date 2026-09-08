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

绑定账号后续不可用时返回明确错误，不静默切换到其它账号。

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
