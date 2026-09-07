# 登录令牌刷新与授权状态

## 判定边界

- Access Token（AT）与 Refresh Token（RT）有不同生命周期；刷新接口一次返回 `401`，不等于 RT 已过期。
- 上游明确返回 `refresh_token_expired`、`refresh_token_reused`、`refresh_token_invalidated`、`invalid_grant` 等失效原因时，仍按原规则报告失败并停止使用对应授权。
- 未分类 `401` 仍向调用方返回刷新错误，但不再把账号永久标为授权失效。诊断信息保留响应类型、请求编号和受限字符集的错误代码，不记录令牌或响应原文。
- 账号可用状态、订阅有效期与额度余量分别管理，本次修复不改变额度或订阅判断。

## 实现约束

1. 网关、用量轮询、手动刷新和 WebSocket 恢复共用 `usage_token_refresh::refresh_and_persist_access_token`，在账号锁内重新读取当前令牌。
2. RT 刷新使用 AT 的 OAuth `client_id`，API 令牌兑换使用 ID Token 的客户端标识，两个 grant 的角色分开解析。
3. 新 AT/RT 与刷新调度先以版本条件原子保存，再进行可选 API 令牌兑换。旧响应遇到版本变化时读取最新状态，不覆盖新登录，也不重新插入已删除的令牌记录。
4. API 缓存只更新自己的字段，WebSocket 清理缓存也使用同一版本条件，禁止整行回写旧 AT/RT。
5. RT grant 不是幂等请求。网络请求可能已发送后发生的断连或超时不盲目重放；只有连接建立失败可以重建客户端再发起请求。
6. 有效 AT 导出时不强制刷新；`auth.json` 的 `last_refresh` 保留实际刷新时间，而不是导出时间。

## 与独立 Codex 客户端同时使用

本项目的账号锁只协调当前服务进程，不能约束其他程序或其他机器。不要让两个独立程序长期共用同一份 RT：一方轮换后，另一方的副本可能变旧。需要同时使用时，应分别建立独立 OAuth 登录，而不是反复复制旧 `auth.json`。

Codex 会维护并回写自己的登录缓存；官方资料也明确要求串行使用一份登录文件，并保留轮换后的新文件，不能反复用旧副本覆盖。[官方登录缓存维护说明](https://learn.chatgpt.com/docs/auth/ci-cd-auth)

已经被上游撤销或过期的 RT 需要重新授权，本次修复不会伪造有效期，也不会修改本机 Codex 的登录文件。

## 回归验证

从 `backend/` 执行：

```powershell
cargo test -p codexmanager-service --lib tokenLifecycleTests --locked
cargo test -p codexmanager-service --lib profileExportTests --locked
cargo test -p codexmanager-service --lib usage:: --locked
cargo test -p codexmanager-service --lib token_exchange --locked
```

回归包含未知 401 与明确失效的区分、旧快照写入拒绝、四线程只消费一次 grant、客户端标识选择、提交后的断连不重放、派生兑换前的持久化可见性，以及导出刷新时间真实性。测试只使用本地 HTTP 端点和虚构令牌。
