# 当前网关与官方 Codex 不一致项

只保留当前最需要继续处理的请求头差异。

## `/v1/responses` 请求头

| 字段 | 官方 Codex | 当前网关 | 当前差异 |
| --- | --- | --- | --- |
| `Authorization` | `Bearer <官方账号 token>` | `Bearer <当前账号 token>` | 网关会替换账号 token |
| `User-Agent` | `codex_cli_rs/<编译时版本> (<os/version; <arch>) <terminal>` | `codex_cli_rs/<数据库配置版本> (<os/version; <arch>) <terminal>` | 官方版本号来自 `env!("CARGO_PKG_VERSION")`，我们当前改成了数据库可配；最终值可手动同步，但来源不一致 |
| `x-client-request-id` | 固定等于 `conversation_id` | 优先等于线程锚点 | 切号切线程时会变成新的线程锚点 |
| `session_id` | 固定等于 `conversation_id` | 优先等于线程锚点 | 普通 `/responses` 没有线程锚点时不再发送 |
| `x-codex-turn-state` | 同一 turn 内回放 | 同一线程稳定时回放 | 切号或线程换代时会主动丢弃 |

## 当前结论

1. 现在最值得继续收的差异就是这 5 项请求头/传输层行为。
2. `gatewayOriginator` 设置值目前仍会保留在本地配置里，但已经不再影响实际出站 `originator`，真实出站固定为官方默认值 `codex_cli_rs`。
3. `User-Agent` 版本号这一项，官方来源是编译时包版本；当前网关为了方便手动追平官方，改成了数据库字段可配。

## 源码依据

- 官方 `codex`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\client.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\requests\headers.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\default_client.rs`
- 当前网关
  - [transport.rs](../../../crates/service/src/gateway/upstream/attempt_flow/transport.rs)
  - [codex_headers.rs](../../../crates/service/src/gateway/upstream/headers/codex_headers.rs)
  - [runtime_config.rs](../../../crates/service/src/gateway/core/runtime_config.rs)