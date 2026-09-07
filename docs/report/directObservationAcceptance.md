# 官方直连观测验收记录

## 总目标与边界

任务来源：会话 `01a07967-7b18-7382-9a43-77a85db5fc4a`。目标保持进行中，本文不是完整交付声明。

- Codex 保持官方 provider、上游地址与登录身份，不使用 Manager `/v1` 网关或账号池。
- Manager 内置观测、自动覆盖已运行与后续启动的 Codex，启用状态跨 Manager 重启保留。
- 原请求成功；请求日志、实际生成 Token、模型价格快照有对应证据；不扣钱包或平台密钥额度。
- DLL 从本仓库构建并随包释放；停用、异常退出和资源回收都需验证。

## 分层验收矩阵

| 边界 | 实现/测试位置 | 目前证据 |
| --- | --- | --- |
| DLL 与配置定位 | `runtimePaths.rs` / `runtimePathsTests.rs` | 明确路径覆盖与安装资源路径；配置原子替换；停用端口为零 |
| Relay TCP/ClientHello | `relayIngress.rs` / `relayIngressTests.rs` | 跨 TCP 分包、跨 TLS record、前缀回放、非法头检测 |
| 官方 WebSocket | `websocketRelay.rs` / `websocketObservation.rs` | 官方生成成功，用量与 CLI 对应，预热单独识别 |
| 官方 SSE | `streamObserver.rs` / `streamObserverTests.rs` | WebSocket 故障后 CLI 回退 SSE，缺少 SSE Content-Type 时仍能从 framing 提取 usage |
| 原子写库与价格快照 | `storage/observationRecords.rs` / `observationRecords.rs` 测试 | 去重、未知价格、非生成预热隔离、钱包账目不变 |
| 展示 | `page.tsx` / `requestProtocol.ts` | 不再遮罩直连统计；WebSocket 与预热标签不误标为 HTTP |
| 完整自动接管 | `processInjector.rs` / `directObservation/mod.rs` | **未验收**：DLL 就绪、已建连接、目标 TLS 信任和重启恢复仍需完整运行证据 |

## 2026-09-07 真实请求证据

使用现有 `codex.exe`，模型 `gpt-5.6-sol`，不复制或输出登录凭据，不覆盖 provider/base_url。
测试仅为新建 CLI 进程配置观测 HTTP 代理和 `CODEX_CA_CERTIFICATE`；原目标为官方 `/backend-api/codex/responses`。
测试数据库为新建空库，位于忽略目录 `backend/target/observationAcceptance…`，没有 Manager 账号池和平台密钥。

| 用例 | 请求分类 | 输入 | 缓存输入 | 输出 | 生成费用快照 | 钱包扣费 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| WebSocket | 1 预热 + 1 生成，0 握手失败 | 19756 | 10752 | 8 | 1 | 0 |
| SSE 故障恢复 | 7 握手失败 + 1 SSE 生成 | 20062 | 10752 | 8 | 1 | 0 |

两例均由测试代码断言 CLI `turn.completed.usage` 与生成记录的输入、缓存、输出相等，且日志没有平台密钥/账号池归属。
SSE 用例使用**仅测试实例**的空 WSS 信任集合造成真实握手失败，以测试 CLI 自身回退；HTTP 上游保留公共根校验。
失败握手没有伪造 Token 或费用快照；表内用量仅为生成请求。

这证明 TLS/协议/数据库边界，不证明用户无需配置代理的自动接管，也不证明正在使用的进程可热接入。

## 实测暴露并修正的问题

1. 宿主 debug 目录写 `hook.json`、DLL release 目录读配置：固定运行期模块及配置路径，不在停止时重新猜测。
2. Rustls 同时启用多个加密实现使 WSS 构造 panic：显式选择 ring 和公共可信根，不关闭证书校验。
3. 一次 `peek()` 猜完整协议头：改为完整读取固定头、rustls 标准握手解析和字节回放。
4. 非白名单 Relay 目标被丢弃：按私有头的原 IP/端口透传，取消仍由同一运行期控制。
5. SSE Content-Type 缺失时全部用量为空：依据解压后 framing 识别 SSE，不改写客户端原响应。
6. 预热 Token 混入生成统计：以真实 `response.create.generate=false` 关联响应，不用零输出猜预热。
7. 直连统计遮罩及 WebSocket 标签错误：恢复直连可见性，统一协议标签。
8. 生命周期测试吞掉数据库清理错误：改为父进程在子进程退出后严格删除自身测试文件。

`generate=false` 是准备状态而非产生模型输出的请求，参考 [官方 WebSocket 说明](https://developers.openai.com/api/docs/guides/websocket-mode)。
预热的原始 usage 保留在请求明细，通过 `usage_included=0` 与生成趋势分离；没有可核对的预热计价依据时不创建生成价格快照，这不表示官方预热必然免费。

## 复现

```powershell
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation:: -- --test-threads=1
cargo test --manifest-path backend/Cargo.toml -p codexmanager-core --test observationRecords
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --test observationLifecycle
pnpm -C frontend run test:runtime
pnpm -C frontend run build:desktop
```

真实流量探针默认忽略，显式执行时提供以下**测试进程**环境变量：

- `OBSERVATION_TEST_CLI`：实际 CLI 绝对路径。
- `OBSERVATION_TEST_DIRECTORY`：新的绝对目录，已存在则失败，避免污染已有数据。
- `OBSERVATION_TEST_MODEL`：可用的原模型名称。
- `OBSERVATION_TEST_UPSTREAM_PROXY`：测试宿主需要网络出口时设置，不含账户凭据。
- `OBSERVATION_TEST_PROTOCOL`：`websocket` 或 `sse`。

```powershell
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

该探针不调用自动扫描，不注入编辑器或现有桌面进程，不修改正式数据库、系统代理和系统证书库。
自动接管阶段须另行覆盖原进程、新进程、Manager 重启、主动停止和异常退出，全部通过后才能标记总目标完成。

## 原生加载阶段（2026-09-07）

已实现 `nativeInjection.rs` 的加载事务，分别验证：

- 进程身份使用原生创建时间，不把 PID 复用当成原进程；架构不同拒绝注入。
- 按 `LoadLibraryW` 实际拥有模块的 RVA 查找目标入口，不复用宿主 ASLR 地址。
- 参数内存和进程/线程句柄统一管理；超时任务继续持有内存，远程线程结束后再回收，并阻止重复加载。
- 就绪事件升级为 `ObservationHookReady2`，同时绑定 PID 与 DLL 路径；旧事件不再被当作当前模块成功证据。
- DLL 去掉画像、注册表、系统代理改写和子进程终止逻辑，网络入口拆分至 `windowsRuntime.rs`。
- trampoline 先发布、入口后启用；全部网络入口完成前不改连 TCP；同一 socket 的私有头只提交一次。

隔离加载测试 `readyModuleAndRepeatedLoad`、`loadedModuleWithoutReadyIsFailure`、
`staleIdentityIsRejectedBeforeLoad`、`timedOutLoadIsNotDuplicatedAndEventuallyReaped` 均通过。
生产 DLL 的 Release 构建也在**零 Relay 端口**下通过隔离加载/重复加载测试，未注入用户桌面或编辑器。
测试夹具不安装任何 hook；其故意延迟 loader 回调的逻辑只位于 `tests/observation/readyFixture.rs`，不随正式包分发。

```powershell
cargo build --manifest-path backend/Cargo.toml -p codexmanager-service --example observationReadyFixture
# OBSERVATION_TEST_READY_DLL 指向上述测试 DLL，分别运行四个 ignored 测试，勿把 fixtureTarget 作为父用例执行。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::nativeInjection::tests::readyModuleAndRepeatedLoad -- --exact --ignored --nocapture --test-threads=1
cargo test --manifest-path backend/Cargo.toml -p codexmanager-direct-common -p codexmanager-direct-hook
```

**未完成项不变**：这些证据仅覆盖加载与就绪，不证明现有 TLS 连接已被捕获；自动 TLS 接入、启动竞态、持久化恢复和实际安装更新仍待完成。
