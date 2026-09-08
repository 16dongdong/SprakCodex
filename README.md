# CodexManager

CodexManager 是面向个人使用的 Codex 多账号管理与会话分流工具。

## 核心能力

- 账号管理：导入 `/api/auth/session` JSON、`auth.json`，完成浏览器授权、令牌续期、额度查询和账号启停。
- 本机接入：通过随安装包发布的 DLL 将本机 Codex 连接接入 CM，不修改 `auth.json`、provider 或 `base_url`。
- 会话分流：默认关闭；开启后按 `thread-id` 为新会话均衡分配账号，同一会话持久绑定同一账号。
- 请求日志：记录实际使用账号、会话标识、协议、模型、耗时、Token、费用估算，以及脱敏后的完整请求和响应。
- 网络与诊断：管理个人网络出口、桌面生命周期、更新、外观和本机诊断日志。

Codex 自己继续管理会话、项目、上下文、模型、Skills 和插件。CM 不提供平台 Key、成员、钱包、充值、聚合 API、模型路由或项目启动页面。

## 页面

```text
账号
请求日志
设置
```

## 开发验证

```powershell
pnpm -C frontend run lint
pnpm -C frontend run test:runtime
pnpm -C frontend run build:desktop
cargo test --manifest-path backend/Cargo.toml --workspace
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib
```

Windows 安装包：

```powershell
pwsh -File backend/scripts/rebuild.ps1 -Bundle nsis
```

详细会话分流设计见 [`docs/sessionRouting.md`](docs/sessionRouting.md)。
