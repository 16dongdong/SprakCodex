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
- 就绪事件使用版本化名称，同时绑定 PID 与 DLL 路径；旧事件不再被当作当前模块成功证据。当前版本为要求运行实例校验与额外 CA 读取入口的 `ObservationHookReady4`。
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

## 签名身份连续性与逐请求独立核对（2026-09-07）

Windows 运行期将 CA 签名密钥保存为当前用户范围的 `observationAuthority.dpapi` 密文。
私钥明文只在内存中用于签名，解密临时缓冲会清零；公开证书使用固定路径 `observationAuthority.pem`，
正常停用或启动失败时保留公开文件，避免客户端重建连接时读取到已删除文件。
这不安装任何系统根证书，也不读取或修改 `auth.json`。
已有密文解密失败时明确报错，禁止自动换一把根密钥；跨 Windows 用户迁移需单独处理，不能把密文文件当通用明文备份。

通过的本地测试包括：重复读取保持同一公钥、磁盘数据不是可解析的 PKCS#8 明文、损坏密文不轮换身份，
以及**旧客户端信任锚验证重新创建的 Authority 签发的新叶子**。不同签名身份仍被拒绝。
这仅证明信任身份连续性，不证明完整 Manager 重启恢复：监听端口、已有连接与自动信任接入仍待验证。

网络探针增加独立的逐请求核对，不再仅比较整轮累计值：

- 只读取本次 CLI 生成的会话文件，从 `token_usage_record` 提取 `response_id` 和 usage。
- 按 `turn_id` 读取对应模型，其他正文、工具参数和推理事件不进入验证结果。
- 将客户端 `response_id` 与网络记录的去重标识逐条匹配，验证每条请求的模型和输入、缓存、输出、总 Token。
- 实测 WebSocket：1 次生成、1 次预热；生成输入 **20630**、缓存 **11392**、输出 **8**，费用快照 **1**，扣费 **0**，逐请求核对通过。

`nativeUsageVerifier.rs` **仅为测试验证器**，没有变更生产统计数据源。
常规 CLI 的逐请求事件已验证可用；`--ephemeral` 不持久化会话文件，原生记录也不提供完整 HTTP 状态和连接细节。
这些局限保留在总验收中，不能通过改用文件记录来宣称所有网络模式均已自动接管。

## Relay 运行实例失效与新连接恢复（2026-09-07）

`directCommon/relayContract.rs` 统一宿主和 DLL 配置，`runtime_owner` 包含进程 ID、
实际 Relay 线程 ID 和原始 FILETIME 创建时间。`runtimeLease.rs` 只打开查询/同步权限的线程句柄，
每次新连接决策进行零超时等待；线程退出、宿主硬退出或身份不匹配时停止新连接改连。
运行实例校验最初使用 `ObservationHookReady3`，额外 CA 读取入口加入后升级为 `ObservationHookReady4`。

`directHook/relayControl.rs` 删除永久启动缓存与 mtime 端口缓存：完整配置限制为 64 KiB，
相同字节复用解析结果和线程句柄，读取失败、删除、损坏、无 owner 或显式停用均清除旧配置。
DLL 可先于配置加载并保持原连接，配置随后生效；同一快照同时提供端口和本地代理列表。
`cleanupRunning` 在取消 listener 前先发布停用配置；发布失败仍回收线程，由线程退出使旧配置失效。
这些操作不修改客户端 provider、base_url、登录身份、系统代理或系统证书。

已验证：

- Common 7 项、DLL 7 项测试通过；包含独立宿主进程硬退出、线程 ID 身份错误、相同 mtime 更新、
  缺失配置后启用、配置删除/损坏/超限、缓存线程退出和新实例重新启用。
- 生产 Release DLL 通过 `productionModuleHonorsRuntimeLifetime`：注入**本测试创建的客户端**，
  只访问两个本测试创建的回环 listener。同步 `connect` 与 Tokio 异步 `ConnectEx` 分别实测
  无配置、启用、原线程退出、重新启用、停用、再次启用、配置损坏、配置删除，共 16 次真实 socket 往返，
  由实际接收连接的 listener 和固定请求/响应字节确认路由，没有将就绪事件当作网络成功证据。
- 测试结束回收客户端、输出线程、监听 socket、独占 DLL、配置和诊断文件。
- 服务观测单元测试 28 项、启用选择持久化测试 1 项、四项独立加载/超时回收测试通过；
  Tauri 独立工作区 `cargo check --manifest-path frontend/src-tauri/Cargo.toml --lib` 通过。

```powershell
cargo test --manifest-path backend/Cargo.toml -p codexmanager-direct-common -p codexmanager-direct-hook
cargo build --manifest-path backend/Cargo.toml -p codexmanager-direct-hook --release --target-dir backend/target/observationBuild
$env:OBSERVATION_TEST_NETWORK_DLL=(Resolve-Path backend/target/observationBuild/release/cphook.dll).Path
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::relayRoutingTests::productionModuleHonorsRuntimeLifetime -- --exact --ignored --nocapture --test-threads=1
```

**范围边界**：这里验证的是新连接路由，不证明已建立 TLS 连接的解密或无损迁移；线程退出与
实际 Winsock 调用之间仍存在执行时间窗口，已建立到 Relay 的连接也不会被迁移到另一服务器。
官方 CLI 自动 TLS 信任接入、快速启动首请求、Manager 完整重启恢复以及安装更新仍未完成，
本次测试不使用显式代理环境伪称自动官方流量验收通过。

## 进程内额外 CA 与官方 CLI 真实请求（2026-09-07）

依据本机 `codex-cli 0.153.4` 对应的官方标签 `rust-v0.153.4`，固定源码提交
`3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`：

- [`http-client/src/custom_ca.rs`](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/http-client/src/custom_ca.rs)
  在构造 TLS 客户端时读取 `CODEX_CA_CERTIFICATE`，未设置或为空时再读取 `SSL_CERT_FILE`；自定义根追加到原信任基础。
- [`websocket-client/src/lib.rs`](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/websocket-client/src/lib.rs)
  在创建 WebSocket connector 时构建该 TLS 配置；已创建的 TLS 连接不会追溯改变。
- `exec/src/lib.rs` 的无位置参数路径输出 stdin 等待标记；显式 `-` 静默等待。因此原生探针省略位置参数，
  等待实际标记后加载生产 DLL，再交付固定无工具请求，不用猜测的 sleep 冒充初始化边界。

`directHook/trustProvider.rs` 只接入 `GetEnvironmentVariableW` 对该固定 CA 变量的读取；
其他名称调用原入口。`trustBundle.rs` 用标准 PEM 解析器提取并合并原自定义 CA 和观测 CA，
不复制私钥块，不写原文件，不修改进程环境块、系统根证书、provider、base_url 或登录身份。
公开文件以不可变内容缓存，单个源文件最多 1 MiB；原路径在客户端进程寿命内保持有效，
多次证书内容更新不会因固定版本数上限停止接入。`FILE_FLAG_DELETE_ON_CLOSE` 负责正常及硬退出时清理。
准备失败时清除新连接接入标记并记录静态诊断；已有 TLS 客户端不由这个读取入口强制重建。

新增探针选择 `OBSERVATION_TEST_CAPTURE_MODE=injected`：

- 测试脚本不设置该 CLI 的 `HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY` 或 CA 环境变量；
  由生产 DLL 提供额外 CA 并改连。官方内置 provider 未覆盖，CLI 使用原登录，观测宿主只转发原认证。
- `OBSERVATION_TEST_ORIGINAL_PROXY_PORTS=7890` 显式描述本机已有代理端口，不改变 CLI 的代理选择。
  **这个列表目前由探针提供，生产自动发现原代理仍待补齐。**
- 最新生产 DLL 的原生 WebSocket 实测：2 条记录（1 次生成、1 次预热），握手失败 0，输入 20314、缓存 11392、输出 8；
  原生 SSE 实测：输入 20314、缓存 11392、输出 8。两者每个响应 ID、模型和 Token 均与独立客户端记录核对通过，
  各生成费用快照 1 条，钱包扣费均为 0。
- SSE 用例仅让探针的上游 WSS 校验失败，以触发官方 CLI 的协议回退；7 次失败握手均被区分记录，没有伪装成生成。
- 单元覆盖名称匹配、UTF-16 缓冲区边界、原变量返回值/LastError、合并原 CA、排除私钥块、坏 PEM、大小限制、
  32 次公开内容更新及旧路径寿命。真实 CLI 退出后核对公开文件已由内核删除，测试模块和配置也已回收。
- 当前 Common 7 项、DLL 13 项、服务观测 28 项通过；生产 DLL 的 16 次 socket 路由回归及 Tauri 独立检查通过。
  最新独立证据目录为 `backend/target/observationInjected0ee5f59b0de54551985409372a49a00e`（WebSocket）和
  `backend/target/observationInjected8ca9fd26b63a44d98036055b20b70515`（SSE），均未加入 Git。

```powershell
$env:OBSERVATION_TEST_CAPTURE_MODE='injected'
$env:OBSERVATION_TEST_NETWORK_DLL=(Resolve-Path backend/target/observationBuild/release/cphook.dll).Path
$env:OBSERVATION_TEST_ORIGINAL_PROXY_PORTS='7890'
# 其余 CLI、独占目录、模型、上游和协议参数沿用本报告的官方流量探针说明。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

**总目标仍未完成**：stdin 边界验证不等同于常驻扫描及时接入所有新进程；已经建立的 TLS/WebSocket、
快速启动首请求、原代理自动发现、完整 Manager 重启恢复、长时间运行的叶子证书续期和安装更新仍需实现或验收。
本阶段证明进程内 TLS 读取接入能完成真实官方请求，没有把显式设置代理/CA 当作原生接入结果。
