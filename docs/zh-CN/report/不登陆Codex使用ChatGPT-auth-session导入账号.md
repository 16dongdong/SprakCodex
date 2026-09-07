# 不登陆 Codex 使用 ChatGPT 的 /api/auth/session 在软件中的使用

本文说明一种不走 Codex 登录授权弹窗的导入方式：先在浏览器里读取 ChatGPT 当前登录会话的 `https://chatgpt.com/api/auth/session` JSON，再把整段 JSON 粘贴到 CodexManager 的“批量导入”里。

> 注意：`/api/auth/session` 返回内容里包含可用于访问账号的敏感 token。只在自己的本机环境复制粘贴，不要发到 Issue、群聊、截图或日志里。本文配图均为脱敏示意。

## 适用场景

- 浏览器里已经登录 ChatGPT。
- 不想在 CodexManager 里重新走一次 Codex 授权登录。
- 只需要把当前 ChatGPT 会话导入到软件账号池里使用。

如果你希望软件长期自动刷新账号，优先使用软件内置“登录授权”。`/api/auth/session` 页面有时只返回当前 `accessToken`，不一定包含可长期刷新的 `refreshToken`；这种账号在 token 过期后可能需要重新导入或重新登录。

## 操作步骤

1. 在已经登录 ChatGPT 的浏览器中打开：

   ```text
   https://chatgpt.com/api/auth/session
   ```

2. 页面会显示一整段 JSON。按 `Ctrl+A` 全选，再按 `Ctrl+C` 复制全部内容，不要只复制其中一小段 token。

   ![ChatGPT auth session 页面复制示意](../../../assets/images/session.png)

3. 打开 CodexManager，进入“账号管理”，点击“新增账号”。

4. 切换到“批量导入”，把刚复制的整段 JSON 粘贴到“账号数据”输入框。

   ![CodexManager 批量导入账号示意](../../../assets/images/import.png)

5. 点击“开始导入”。导入完成后，在账号列表刷新用量，确认账号状态可用。

## 支持的字段形态

批量导入会自动识别常见字段名。`/api/auth/session` 的整段 JSON 通常包含 `accessToken`，软件会按兼容格式解析；如果 JSON 中还带有 `refreshToken`、`idToken` 或账号 ID 信息，也会一并使用。

只有有效 `accessToken` 也可以导入。首次请求 Codex 上游或执行账号预热时，CodexManager 会按需注册并持久化该账号缺少的 AgentIdentity，随后使用 AgentAssertion 鉴权；不需要从 `/api/auth/session` 手工寻找或填写 AgentIdentity。软件也能识别完整的嵌套 `agent_identity` / `agentIdentity` 对象及 snake_case、camelCase 字段，但不会把单独的 JWT 字符串当成可信的 AgentIdentity 记录。

账号显示名会优先读取 JWT 的 OpenAI profile 邮箱，其次读取会话 JSON 中的 `user.email` / `user.name`。这些字段只用于显示，不参与权限判断；已经手工修改的账号名称不会被自动覆盖。

可识别的常见字段包括：

```json
{
  "accessToken": "eyJ...",
  "idToken": "eyJ...",
  "refreshToken": "rt_...",
  "accountId": "acc_..."
}
```

也支持下划线格式：

```json
{
  "access_token": "eyJ...",
  "id_token": "eyJ...",
  "refresh_token": "rt_...",
  "account_id": "acc_..."
}
```

实际复制时不需要手动改字段名，直接粘贴 `https://chatgpt.com/api/auth/session` 页面返回的完整 JSON 即可。

## 常见问题

### 打开页面不是 JSON

先确认浏览器已经登录 ChatGPT。未登录、登录过期、网络被拦截时，页面可能返回登录页、错误页或空内容，需要重新登录 ChatGPT 后再刷新。

### 导入失败提示 JSON 格式不正确

通常是没有复制完整 JSON，或复制时带入了浏览器额外文本。重新打开页面，使用 `Ctrl+A`、`Ctrl+C` 复制整页内容后再粘贴。

### 导入后请求仍返回 401 Unauthorized

先确认已经更新并重启到包含 AgentIdentity 自动注册修复的 CodexManager。当前版本会为仅含有效 `accessToken` 的账号按需补齐 AgentIdentity；如果仍失败，再检查 accessToken 是否已经过期，以及代理或网络是否拦截了 AgentIdentity 注册请求。

如果导入内容没有 `refreshToken`，accessToken 过期后仍无法自动续期。此时重新复制 `/api/auth/session` 再导入，或改用软件内置“登录授权”。

### 提示 model is not supported

这是当前 ChatGPT 账号计划不具备该模型权限，不是 token 或 AgentIdentity 鉴权失败。本地模型目录中存在某个模型，不代表当前账号一定能调用它；请改用该账号上游实际允许的模型。

### 能不能把这段 JSON 发给别人帮忙排查

不能。它和密码、Cookie、Refresh Token 一样敏感。排查问题时只能贴脱敏后的字段结构或错误信息，不要贴真实 token。
