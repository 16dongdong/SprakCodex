# 官方直连观测验收记录

## EXE 内嵌载荷与内存部署（2026-09-09）

- `codexmanager-service/build.rs` 构建 `codexmanager-direct-hook`，将 PE 字节复制到当前 `OUT_DIR`；`runtimePaths.rs` 使用 `include_bytes!` 链接到宿主 EXE。
- Windows 运行期直接解析内嵌 PE，完成区段映射、DIR64 重定位、系统依赖加载、IAT 地址解析、x64 异常函数表注册、最小页面权限设置与显式初始化入口调用。
- 安装资源清单已移除 `observationHook9.dll`，Relay 控制由版本化命名页映射发布，不再生成安装目录 JSON 控制文件。
- 回调状态容器改用 `BTreeMap`／`BTreeSet`，避免手工映像依赖 Windows loader 分配静态 TLS；生产 Winsock 夹具覆盖同步 `connect`、Tokio `ConnectEx`、启停、宿主租约失效和再次启用。
- 第十二版新增 `observationShutdown`：看门狗按 PID 与创建时间调用固定卸载 ABI，模块恢复 detour、排空回调、释放 trampoline／证书／映射句柄、执行 CRT detach 并注销 x64 异常表，宿主随后验证主映像已释放。
- `updateAgent.exe watchdog` 在父进程私有管道关闭后执行全部记录的卸载；更新作业由同一看门狗持有，卸载失败时不会继续安装或叠加新映像。

```powershell
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service directObservation::relayRoutingTests::productionModuleHonorsRuntimeLifetime -- --ignored --exact --nocapture
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service directObservation::nativeInjection::tests::readyModuleAndRepeatedLoad -- --ignored --exact --nocapture
```

后续章节保留各历史阶段的原始验收命令和版本信息，用于回溯迁移前行为；当前发布边界以上述第十二版内存部署与看门狗结果为准。

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
| 常驻发现与接入 | `processMonitor.rs` / `processInjector.rs` | 新 CLI 无 stdin 同步点的双协议真实请求通过；已建连接和完整重启恢复仍需运行证据 |

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
- 就绪事件使用版本化名称，同时绑定 PID 与 DLL 路径；旧事件不再被当作当前模块成功证据。当前版本为同时要求运行目录元数据发布的 `ObservationHookReady6`。
- DLL 去掉画像、注册表、系统代理改写和子进程终止逻辑，网络入口拆分至 `windowsRuntime.rs`。
- trampoline 先发布、入口后启用；全部网络入口完成前不改连 TCP；同一 socket 的私有头只提交一次。

隔离加载测试 `readyModuleAndRepeatedLoad`、`loadedModuleWithoutReadyIsFailure`、
`staleIdentityIsRejectedBeforeLoad`、`timedOutLoadIsNotDuplicatedAndEventuallyReaped` 均通过。
生产 DLL 的 Release 构建也在**零 Relay 端口**下通过隔离加载/重复加载测试，未注入用户桌面或编辑器。
测试夹具不安装任何 hook；其故意延迟 loader 回调的逻辑只位于 `tests/observation/readyFixture.rs`，不随正式包分发。

```powershell
cargo build --manifest-path backend/Cargo.toml -p codexmanager-service --example observationReadyFixture
# OBSERVATION_TEST_READY_DLL 指向上述测试 DLL，分别运行七个 ignored 测试，勿把 fixtureTarget 作为父用例执行。
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

该阶段的 `nativeUsageVerifier.rs` **仅为测试验证器**；后续生产客户端事件来源在文末单独说明。
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
- 该阶段最初由探针显式提供已有代理端口；下面的代理自动发现阶段已移除此输入和对应配置字段。
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
# 其余 CLI、独占目录、模型、上游和协议参数沿用本报告的官方流量探针说明。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

**总目标仍未完成**：stdin 边界验证不等同于常驻扫描及时接入所有新进程；已经建立的 TLS/WebSocket、
快速启动首请求、其余代理传输边界、完整 Manager 重启恢复、长时间运行的叶子证书续期和安装更新仍需实现或验收。
本阶段证明进程内 TLS 读取接入能完成真实官方请求，没有把显式设置代理/CA 当作原生接入结果。

## 原代理自动识别与双地址族 Relay（2026-09-07）

`proxyDiscovery.rs` 在目标进程的新本地连接上读取其 HTTP/HTTPS/ALL 代理变量，
`systemProxy.rs` 通过只读的
[`WinHttpGetIEProxyConfigForCurrentUser`](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpgetieproxyconfigforcurrentuser)
读取当前用户活动网络的静态代理配置，并按 API 契约释放三个返回字符串。
没有端口扫描、系统设置写入或 PAC/WPAD 请求；不保存原代理 URL 或认证值。

- 移除 `blocked_loopback_proxy_ports` 及测试端口输入：生产与真实 CLI 探针使用相同的自动发现路径。
- 同时匹配 IP/localhost、端口与代理传输类型；`https=…` 是请求协议选择器，不误当作 TLS 到代理。
- 使用 URL/Host 标准解析与哈希集合，处理非特殊协议中的 IP 规范化，避免大量候选项的二次复杂度去重。
- 普通本地服务、远端地址、坏配置、SOCKS、HTTPS 代理、带认证或同端点协议冲突不加入本地明文接入集合。
  这些未接入的代理边界继续属于完整观测的待实现项，不计作已覆盖。
- 不使用过期端口缓存，每次本地新连接读取当前设置，逐包发送路径不查询代理。

`loopbackListeners.rs` 在同一个端口绑定 `127.0.0.1` 和 `::1`，共同使用连接配额与任务组，
全部绑定完成才发布配置。只对跨地址族端口碰撞重新选端口，其他绑定错误直接使启动失败；
不监听通配地址。由此修复 IPv6 socket 改连 `::1`、宿主却仅监听 IPv4 的接口不一致。

通过的验证：Common 7 项、DLL 17 项、服务观测 29 项、Tauri 独立工作区检查、Release DLL 构建；
真实生产 DLL 的 IPv4/IPv6 × connect/ConnectEx 生命周期共 32 次 socket 往返，配置开启、退出、
恢复、停用和损坏行为一致。独立 listener 测试确认两个地址族共用端口并一起释放。
最新官方 CLI 的 WebSocket/SSE 测试均不再提供原代理端口列表，也不设置该 CLI 的代理或 CA 环境变量；
两种协议生成成功，响应 ID、模型、Token 逐条对应，费用快照各 1 条，钱包扣费均为 0。
最新 WebSocket 输入 20642、SSE 输入 20312，缓存输入均为 11392、输出均为 8。
证据目录分别为 `backend/target/observationAutoProxy78208e21a4a445a1a6a0a2f7718f7223` 和
`backend/target/observationAutoProxy229f9218a97b4e8299894072594fac43`；测试进程和公开证书残留数均为 0。

**剩余边界**：自动发现目前覆盖目标环境和 Windows 静态设置中的本地明文 HTTP 代理；
PAC 动态结果、其他代理协议及观测器上游路线的逐客户端保持仍未闭环。
这些结果也不代替常驻进程扫描、已建立连接、完整重启恢复、证书续期和实际安装更新的验收。

## 生产常驻扫描的无输入等待验证（2026-09-07）

`processMonitor.rs` 从启动入口提取常驻发现/加载循环，生产与探针共用这一实现。
同步系统目录枚举移到阻塞工作线程；已加载集合按完整进程实例维护，取消后不开始新枚举。
首轮立即扫描；初始单进程测试仍使用两秒周期，随后连续进程测试复现遗漏，并按下节证据改为轻量目录与 50 ms 周期。

`OBSERVATION_TEST_CAPTURE_MODE=monitored` 的执行顺序：先完成一轮真实目录扫描，再启动带完整位置提示词的 CLI；
其 stdin 为关闭状态，不等待标记、不延迟提交提示词，也不直接调用注入函数。
生产扫描器自行发现并加载模块。测试候选仅按自建进程的 PID、创建时间和可执行文件限定，
不把用户其他 CLI 会话纳入测试；这一隔离选择不改变生产的全目录发现逻辑。

- WebSocket：2 条记录（1 次预热、1 次生成），输入 20640、缓存 11392、输出 8，费用快照 1，扣费 0。
  证据目录：`backend/target/observationMonitoredBaseline7613843fbf9b4117b9ea67bdbf0ffeb9`。
- SSE：7 次失败握手后生成成功，输入 20642、缓存 11392、输出 8，费用快照 1，扣费 0。
  证据目录：`backend/target/observationMonitoredSse0f7cd67e102c481ba7ddc6831d2f535e`。
- 两例模块加载阶段均约 107 ms，生成响应 ID、模型和逐请求 Token 与客户端记录对应。
当时单进程测试未复现首请求遗漏；它不证明所有启动时序与多进程竞争都已覆盖，下节连续用例随后复现了遗漏。
- 单元覆盖首轮立即发现、预先取消不枚举，以及取消后及时退出；进程实例限定避免测试 PID 复用越界。

本机普通用户令牌查询 `Win32_ProcessStartTrace` 返回 `PermissionDenied`，未修改系统权限或常驻订阅。
只读 Toolhelp 快照基准 100 次平均约 9.64 ms，因此没有用高频全目录轮询掩盖事件能力限制。
原会话明确允许技术方案变化，以请求数、Token 与费用快照为验收；后续既有长连接仍须独立取得证据，
不能把本节新进程成功推广成现有连接已被完整观测。

```powershell
$env:OBSERVATION_TEST_CAPTURE_MODE='monitored'
# CLI、独占目录、模型、上游与协议参数沿用真实流量探针；不设置 CLI 代理或 CA 环境变量。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

## 连续进程首请求遗漏的复现与修复（2026-09-07）

单个 CLI 成功不足以证明常驻观测。探针现复用同一个扫描任务，前一 CLI 退出后再启动第二个 CLI，
合并两份 stdout 终态并逐个读取其独立 `response_id` 用量记录；只捕获其中一次将直接失败。
两秒周期下真实复现“两次 CLI 都成功、两次 DLL 都就绪，但观测库为空”，证据位于
`backend/target/observationContinuousaf7b394bf330404f99a21ed92eb502d7`。根因是发现晚于 TLS 客户端构造，
而非模块加载失败；不能用就绪事件替代请求观测证据。

`processCatalog.rs` 按
[`SystemBasicProcessInformation` 的公开 ABI](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation)
读取基本目录，不获取线程资源统计、命令行或环境。只有系统明确返回信息类不支持时选择旧版 Toolhelp API；
权限、内存和格式错误保留为错误，扫描器不把它们当空目录清掉已加载状态。
缓冲区有 16 MiB 上限，校验记录推进、返回长度、Unicode 长度及指针范围，加载前仍核对实际进程创建时间与路径。
本机基本目录查询 100 次共 7032 微秒，平均约 0.07 ms（不包含后续加载），扫描周期改为 50 ms；
加载等待期间跳过旧 tick，避免积压后的突发补扫。

修复后同一连续用例通过：

| 协议 | 生成/预热/失败握手 | 输入 | 缓存 | 输出 | 费用快照/扣费 |
| --- | --- | ---: | ---: | ---: | --- |
| WebSocket | 2 / 2 / 0 | 40958 | 22784 | 16 | 2 / 0 |
| SSE | 2 / 0 / 14 | 40954 | 22784 | 16 | 2 / 0 |

证据目录为 `backend/target/observationContinuousFixed7700144d665a48b08e06c9ac4b2a3079` 和
`backend/target/observationContinuousSsee06008cba1594cafa4c49bb5d415aee0`。
SSE 失败握手仍是测试主动触发协议回退，不是生成请求。34 项服务观测单元测试、生命周期测试与 Tauri 检查通过。
本节修复已复现的新进程启动问题；已有长连接、多目标加载竞争、完整重启和安装更新仍不由这些证据证明。

## 已运行会话的完成事件补充与跨来源合并（2026-09-08）

`warmSessionProbe.rs` 使用实际 `codex app-server`：先完成一轮官方生成，然后启用生产扫描器，
保持进程、thread 和现有连接不变，再执行第二轮。原实现复现“第二轮成功、模块就绪、对应观测缺失”，
失败证据位于 `backend/target/observationWarm97950af9232549a696560564d9cdba8e`。
原会话明确允许技术方案变化，验收目标是请求记录、Token 和费用快照；这里增加补充来源，不用断开连接重试来规避遗漏。

### 生产来源与字段真实性

- `clientEventMonitor.rs` 使用文件变化通知监听当前 CLI home 的 `sessions` 目录，只接受 `rollout-*.jsonl`。
  注册时刻之前的完成事件不回填；目录溢出和 Rescan 通知重新核对目录，读取失败保留位置并限速重试。
- `clientEvents.rs` 流式处理 `session_meta`、`turn_context`、`token_usage_record`，未知正文直接跳过。
  只提取官方 provider 标记、会话/轮次归属、模型上下文和实际 usage；半条 JSON 等待后续字节，缺失计数不补零。
- 完成事件记录为 `clientResponse` / `clientObservation`，模型来源为 `client_context`。
  HTTP 状态、URL、方法、路径和时长没有证据时保持未知；前端明确显示“客户端事件”，不伪装成 HTTP 抓包。
- 网络来源继续保留。两者在 `recordSink.rs` 共用数据库写入队列；客户端读取位置在数据库提交确认后推进，
  失败会重试原事件，而不是提前跳过它。

### 去重与价格快照

`responseIdentity.rs` 使用 `observationResponse:{SHA256(response_id)}` 作为跨来源标识，同时识别旧版两个官方主机的散列。
`insertObservation` 在同一事务内复用请求主键：网络记录可补全客户端元数据，客户端也可补齐网络缺失的 usage；
完整网络字段不被较弱来源覆盖，冲突计数拒绝合并。统计与费用快照不重复增加，已有扣费账目的记录不允许被观测器改写。
缺失价格或未表达的缓存写入计费保持未知，绝不生成零价快照。

### 验收结果

- 已运行会话的第二轮：逐响应 ID、模型、输入/缓存/输出/总量/推理 Token 与 app-server 的 `rawResponse/completed` 独立通知匹配。
  断言未回填第一轮；记录为客户端事件且 HTTP 字段均为空，费用快照 1 条，钱包扣费 0。
  证据：`backend/target/observationEventAcceptance9b81fe7d62a84e0295d03218c263feec`。
- 连续新 CLI 的 WebSocket 及 SSE 网络观测保持通过；最新 SSE 两次生成输入 40640、缓存 28288、输出 16，
  费用快照 2 条、扣费 0，另有测试刻意触发的 14 次失败握手。
  证据：`backend/target/observationEventAcceptance6c936b9ec82e478d94a8094c8f0116cc`。
- 回归包含客户端先到/网络先到合并、历史标识迁移、未知价格、预热隔离、半条事件、提交失败重试、历史过滤及正文跳过。
  前端 224 项测试和静态构建通过；Tauri 独立工作区检查通过。

```powershell
$env:OBSERVATION_TEST_CAPTURE_MODE='warm'
# 保持现有 CLI 登录与 provider；仅在这个无工具探针进程内禁用其 MCP 启动，不写用户配置文件。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

**该阶段后续工作**：当时事件源只使用宿主解析的 CLI home；下一节补齐独立目录的自动注册。
无持久化会话不产生这类文件，已建无持久化长连接仍需其他观测入口。
文件读取位置当前在内存中，完整重启及异常退出边界、多目标并发加载、证书续期和实际安装更新仍需继续验证。
网络测试与本节完成事件测试分别标明来源，不能据此宣称所有场景已完成。

## 运行目录自动发现（2026-09-08）

`directHook/runtimeMetadata.rs` 在目标进程的初始化线程读取 `CODEX_HOME`，未设置时使用默认用户目录。
不读取 `auth.json`，不复制凭据，不写环境或客户端配置。`directCommon/runtimeHome.rs` 用分页文件支持的
只读消费映射发布目录，协议为固定小端头与有界 UTF-16 路径，保留中文及原路径编码。
映射名同时绑定 PID、精确创建时间与模块路径；解码检查头部、身份、未知标志、长度与绝对路径。
重复发布不覆盖已有对象，发布句柄由客户端进程持有，因此宿主重启后可重新读取，客户端退出后由内核回收。

`processMonitor.rs` 在模块就绪后读取真实运行目录，通过 `HomeRegistration` 排入事件线程。
`clientEventMonitor.rs` 按规范化路径去重、注册新目录，并沿用本次观测的起始时间处理其事件。
目录注册失败保留重试并限速，不把入队失败标成已完成；读取和加载不会改写原登录或上游地址。
就绪协议升级为第六版，避免旧模块缺少目录发布能力却被认为已经完成接入。

验证：

- Common 9 项、DLL 17 项测试通过；UTF-16 中文路径、错误进程身份、截断长度、重复发布及映射释放均覆盖。
- 生产 DLL 的 32 次双地址族 socket 回归中，测试进程使用独立中文 `CODEX_HOME`，宿主读取值与该进程环境一致。
- 真实 app-server 保留原登录和配置目录；观察器初始目录刻意指向另一个空目录。
  只有生产扫描器读取进程映射并注册真实目录后，同一已运行会话的第二轮才完成用量与费用快照核对。
  证据：`backend/target/observationHomeDiscovery645aa000d1634e30af58df289adef172`。
  未复制或迁移登录材料；测试结束后空观察目录及进程已清理。
- 四项原生加载/重复加载/错误身份/超时回收回归通过，Release DLL 构建和 Tauri 独立工作区检查通过。

该阶段补齐目录发现，不代表无持久化会话、完整宿主重启恢复、并发加载、证书续期和安装更新已经验收。

## 无持久化会话的原生完成事件 ABI 探针（2026-09-08）

本阶段只增加 `tests/observation/runtimeUsageFixture.rs` 测试 DLL 与 `runtime` 验收模式，
不替换生产 `directHook`，不把探针报告计入正式数据库或费用快照。
目标是验证无会话文件、已有连接保持不变时，是否能读取实际完成用量。

### 构建证据及读取边界

- 本机 CLI 为 Windows x64 `0.153.4`，官方源码标签 `rust-v0.153.4`，提交
  `3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`。
- 官方同版本发布中的 `codex-symbols-x86_64-pc-windows-msvc.tar.gz` 包含 `codex.pdb`。
  其 GUID `{968E0C4A-097B-A80C-4C4C-44205044422E}`、age 1 与本机 EXE 的 CodeView 一致。
  此匹配不等于整个 EXE 文件字节一致；探针另校验目标函数入口的 16 字节。
- 源码 `core/src/state/session.rs::SessionState::record_token_usage` 在可选会话文件持久化前执行。
  PDB 给出的函数 RVA 为 `0x063d91e0`，入口为
  `55 41 57 41 56 41 55 41 54 56 57 53 48 81 ec 08`。
- 每次从当前模块基址重新定位；只在 `OBSERVATION_TEST_CAPTURE_MODE=runtime` 且
  PDB、age、入口字节全部匹配后安装。该布局只验证了此 Windows x64 构建，不推广到其他版本或架构。
- Win64 参数依次为 sret、state、thread UUID、turn 字节指针、turn 长度、session、root-turn String、
  response String、usage。回调保留原参数、返回值和 `system-unwind` ABI。
- String 指针/长度偏移分别为 8/16；usage 的六个 i64 从 `0x18` 起依次为输入、缓存输入、
  缓存写入、输出、推理输出、总量。模型指针/长度位于 state 的 `0x8f8`/`0x900`，
  来自 `previous_turn_settings`，属于客户端上下文而非独立的服务端模型声明。
- 只通过有界本进程读取复制这些字段，不读取认证或消息正文。所有计数必须非负、满足总量关系，
  模型必须等于测试指定值，响应 ID 必须符合预期格式。无效字段不进入报告。

### 真实验收

`warmSessionProbe.rs` 先启动独立 app-server，并以 `ephemeral=true` 创建 thread，断言持久化路径为空。
同一进程/thread 完成第一轮后，生产扫描器才加载测试 DLL，再进行第二轮；不重建 thread、不重登、不强制断开连接。
独立 RPC `rawResponse/completed` 提供比较基准，逐项核对响应 ID、thread、模型与六个用量计数。

首轮验证目录为 `backend/target/observationRuntimeAbi5060435c7c6f48f3a771785f915aca6e`。
增加无持久化路径断言后的复核目录为
`backend/target/observationRuntimeAbiFinal1b89dece20cb485fafdd2fb40c970ed9`：
第二轮一条响应，输入 31095、缓存输入 30848、缓存写入 0、输出 8、推理输出 0、总量 31103，
探针文件与独立 RPC 元数据完全一致。测试 stdout 明确区分“ABI 验证通过”与“生产接入仍需实现”。
测试 DLL 构建通过，观测模块回归 37 项通过、10 项需显式环境的测试跳过；
链接器仅有 MSVC 创建导入库的本地化提示。复核未发现该探针遗留的 CLI 子进程。

```powershell
cargo build --manifest-path backend/Cargo.toml -p codexmanager-service --example runtimeUsageFixture
$env:OBSERVATION_TEST_CAPTURE_MODE='runtime'
$env:OBSERVATION_TEST_NETWORK_DLL=(Resolve-Path backend/target/debug/examples/runtimeUsageFixture.dll).Path
# CLI、全新独占目录、模型、协议及上游参数沿用真实流量探针，不复制登录材料。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

后续生产接入还需有界元数据传输、报告错误与生命周期管理、来源标记、数据库合并和费用验证；
本测试 DLL 的同步文件输出不是生产实现，也不随正式应用部署。

## 慢加载隔离与并发官方流量（2026-09-08）

原 `processMonitor.rs` 在候选循环内逐个等待 `spawn_blocking(inject)`，任一目标的加载或就绪等待
会阻塞后续目录扫描。现将加载改为独立任务集合，最多同时执行 4 项；50 ms 目录扫描不再等待这些任务。
完整进程身份对应 `Loading`、`Ready` 或失败后 1 秒冷却状态，重复目录项和重复扫描不重复派发。
加载中的实例即使退出目录也保留状态直到事务完成；线程异常同样解除加载状态，不永久卡住该实例。

停用后停止派发和目录注册，等待已经启动的本地工作完成或向原生延迟清理器交接。
`mod.rs` 显式等待扫描器退出，再销毁目录消费者和 Tokio 运行时，避免遗漏后台加载的生命周期依赖。
原生超时事务仍沿用原有远程分配所有权，不提前释放目标线程正在使用的参数。

验证结果：

- 调度单元覆盖：预先取消、首轮扫描、慢加载期间发现新实例、停止排空、4 项上限、重复目录去重、
  后续实例最终接入、线程异常后限速重试、同 PID 不同创建时间独立处理。
- 两个真实独立进程使用生产扫描器和原生加载器；第一个 DLL 延迟 3 秒，第二个先就绪。
  测试确认完成顺序与预留释放，不使用替身加载器代替此项证据。
- 新增 `OBSERVATION_TEST_CAPTURE_MODE=concurrent`：先启动生产目录扫描，再启动两个官方 CLI，
  断言它们存在重叠存活窗口；提示词直接随命令提交，不等待 stdin 标记或模块就绪。
  两份 stdout 独立写入、退出后合并，避免并发写造成 JSON 行交错；必须有两份完整终态。
- 每个响应 ID、模型和实际 Token 对应独立客户端用量，官方 provider、原登录及账号池边界不变。

| 协议 | 生成 / 预热 / 失败握手 | 输入 | 缓存 | 输出 | 费用快照 / 钱包扣费 |
| --- | --- | ---: | ---: | ---: | --- |
| WebSocket | 2 / 2 / 0 | 41272 | 22784 | 16 | 2 / 0 |
| SSE | 2 / 0 / 14 | 40632 | 22784 | 16 | 2 / 0 |

证据目录分别为 `backend/target/observationConcurrent552621c3c5434b0ebed81e707a7d47d6` 和
`backend/target/observationConcurrentSsed82aad74aa094aa1a23c70351de9f1af`。
SSE 的失败握手由探针主动触发协议回退，不作为生成或用量计算。
40 项观测回归通过，11 项需显式环境的测试默认跳过；Tauri 独立工作区检查通过。
另外显式执行并发加载、重复加载、缺少就绪、错误进程身份、超时后延迟回收五项原生测试，全部通过。
两次真实流量结束后，独占 DLL 目录、公开证书和拆分 stdout 均已清理；进程目录中没有这些探针的子进程。

```powershell
$env:OBSERVATION_TEST_CAPTURE_MODE='concurrent'
# 使用本次构建的生产 DLL、官方 CLI、模型、上游与全新独占目录。
$env:OBSERVATION_TEST_PROTOCOL='websocket' # SSE 用另一个全新目录单独执行
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

并发加载和两路官方流量已取得上述证据；无持久化完成事件的生产接入、完整宿主重启恢复、
证书续期及已安装应用更新仍需继续，不能由本节结果替代。

## 运行期证书续签与过期批次真实验收（2026-09-08）

`certificateAuthority.rs` 原先只在启动时签发 7 天证书，运行时间超过有效期后，新的 TLS 握手会失败。
现保留原签名身份和固定站点 `ServerConfig`，用 `SiteResolver` 在握手时读取当前证书批次：

- 证书有效期仍为 7 天，提前 1 天续签；维护任务首轮立即检查，之后每小时检查一次。
- 同步密钥生成在阻塞工作线程执行，不持有发布锁签发；所有白名单站点完成并校验公私钥匹配后才整批替换。
- 失败不发布半批证书，错误明确记录，下一检查周期重试；较旧时间的任务不覆盖更新批次。
- 只更新叶子证书与站点密钥，CA 签名身份、公开信任文件、系统根集合、上游校验及已有 TLS 连接不变。
- 维护任务归属 `transport::serve` 的 `TaskTracker`，与连接共用取消和退出等待，不启动独立常驻进程。

`certificateRenewalTests.rs` 六项测试通过：

1. 阈值前不续签，阈值到达后一次更新全部站点，重复检查不重复签发。
2. 原客户端仅加载一次公开根、禁用会话恢复，推进其独立验证时钟到第 8 天；新叶子握手成功，错误主机名仍失败。
   即使原公开 CA 的证书有效期已过去，Rustls 保留的信任锚仍校验新叶子的签名和有效期；不据此推断其他 TLS 库行为。
3. 过期批次在正常时钟下先握手失败，生产维护任务首轮续签后成功；停用后任务退出并释放签名状态引用。
4. 更新批次后，之前建立的 TLS 流继续双向字节往返。
5. 预先取消不进行续签；站点签发部分失败不改变原批次。
6. 较旧时间请求不覆盖新有效期的证书。

真实探针增加 `OBSERVATION_TEST_EXPIRED_CERTIFICATE=true`，仅把本次独立观测器的初始签发时间设为 8 天前。
不调整系统或 CLI 时间、不关闭证书验证、不修改客户端代理/CA 环境。生产维护任务实际输出续签成功后，
两个独立官方 CLI 完成并发生成；客户端使用最初的公开信任文件，不在测试中换发其信任材料。

| 协议 | 生成 / 预热 / 失败握手 | 输入 | 缓存 | 输出 | 费用快照 / 钱包扣费 |
| --- | --- | ---: | ---: | ---: | --- |
| WebSocket | 2 / 2 / 0 | 40628 | 22784 | 16 | 2 / 0 |
| SSE | 2 / 0 / 14 | 40652 | 22784 | 16 | 2 / 0 |

响应 ID、模型、逐请求用量与独立客户端记录对应，SSE 失败握手仍为探针主动触发的协议回退。
证据目录分别为 `backend/target/observationRenewed5a1bc56110a44861affdd6793337b79b` 和
`backend/target/observationRenewedSse5cd7e784e8524ca8b8d7d54216a2ff22`。
46 项观测回归通过，11 项需显式环境的测试默认跳过；Tauri 独立工作区检查通过。

```powershell
$env:OBSERVATION_TEST_CAPTURE_MODE='concurrent'
$env:OBSERVATION_TEST_EXPIRED_CERTIFICATE='true'
# 每种协议使用全新目录，其余官方 CLI、模型、生产 DLL 与上游参数沿用真实流量探针。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::officialTransportRecordsUsage -- --exact --ignored --nocapture --test-threads=1
```

本节验证续签机制、TLS 有效期边界、连接连续性及官方真实请求，并非声称已经实际等待运行 7 天。
无持久化事件的生产接入、完整宿主重启恢复和实际安装更新仍待完成。

## Windows 启动环境与完整宿主进程重启（2026-09-08）

`codex_projects.rs::launch_codex_terminal` 的 Windows 分支仍调用旧 `configureChild`，
将临时观测监听地址写入终端的代理环境，导致宿主退出后终端可能持续连接旧端口。
删除此调用；Windows 由常驻扫描器接入，保留其原代理、CA、provider 和登录环境。
非 Windows 的显式代理入口保留在平台条件编译下，不将本机原生功能推广到未实现的平台。

`directObservation/mod.rs` 提取内部 `RuntimeScope`，只替换进程候选与事件文件范围；
首次启用、DPAPI 身份加载、数据库初始化、监听发布、启用持久化、退出回收仍使用生产实现。
正式入口使用完整范围，不读取测试过滤环境变量。恢复设置读取失败现在明确报错，不当作关闭。

`hostRestartProbe.rs` 执行真实生命周期：

1. 独立官方 app-server 创建 thread 并完成基线生成，随后保持该 PID、创建时间与 thread。
2. 宿主 A 调用生产启动事务；原会话继续生成，逐响应核对用量和费用快照。
3. A 调用 `shutdownRuntime` 正常退出操作系统进程，检查两个地址族的旧监听端口已释放。
4. 全新宿主 B 读取同一测试数据库中的启用设置自动恢复，不重新启用、不复制登录材料。
   其公开 SPKI 指纹与 A 一致，原会话再次生成并通过用量与费用核对。
5. B 主动停用并退出；宿主 C 读取同一设置后保持关闭。原 CLI 仍能生成，记录数不增加。

结果证据：`backend/target/observationHostRestartFinala8f70f5f67914f89ad7f40b277226d79/restartEvidence.json`。
三个宿主分别记录真实 PID 和创建时间，两个被观察到的响应输入为 31667、31695，缓存输入均为 31488，
输出均为 8，费用快照共 2 条、钱包扣费 0。来源为客户端完成事件，HTTP 状态、URL、时长保持未知，
不把客户端事件冒充重启后网络解密结果。测试结束已回收客户端与宿主，删除独占 DLL、DPAPI 密文和空观察目录。

```powershell
# 指定官方 CLI、生产 DLL、原 CODEX_HOME、模型和全新独占目录；不改 auth.json。
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib directObservation::liveDirectTests::hostRestartProbe::existingSessionSurvivesHostRestart -- --exact --ignored --nocapture --test-threads=1
```

前端 225 项运行期回归、静态桌面构建通过；Tauri 76 项测试通过。
全量 Tauri 测试发现旧托盘将主/次窗口重置时间交叉显示，已修正参数对应关系；
备份不覆盖测试改用实际文件字节与账户可读性验证，保留源文件被替换后的不覆盖断言，不再以文件时间戳代替内容证据。

本节证明正常退出、完整新宿主恢复与后续轮次；宿主异常终止时的在途请求、离线事件补采、
无持久化事件生产接入和实际安装更新仍需继续验收。

### 工作区全测未通过项

执行 `cargo test --manifest-path backend/Cargo.toml --workspace -- --test-threads=1`，
服务库结果为 1574 项通过、11 项失败、16 项跳过；工作区命令因此退出 101，不能报告全工作区通过。
完整输出保存在 `backend/target/observationWorkspaceTests.log`。
失败集中于 `http::proxy_runtime::tests` 的 WebSocket 重连、连接限制、发送失败、重放边界、心跳和压缩协商。
单独复跑 `official_responses_websocket_reconnects_upstream_without_closing_client` 仍在
`proxy_runtime_tests.rs:4302` 报 `upstream reconnect frame timeout: Elapsed(())`，不是仅在全套运行时出现。
压缩用例另报告 `Handshake not finished`。这些失败尚未定因或修复，列入后续回归处理，不归因于本次修改或环境猜测。

## WebSocket 回归环境别名冲突修复（2026-09-08）

上述 11 项失败已定位到测试环境初始化，而非重连状态机或压缩协议本身：

- 临时诊断确认目标仍是测试绑定的 `127.0.0.1` 端口，但连接选择了代理；夹具没有收到 TCP 连接。
- 11 个测试都先 `EnvGuard::set("NO_PROXY", "127.0.0.1,localhost")`，随后
  `EnvGuard::clear("no_proxy")`。Windows 环境变量名不区分大小写，后一步删除了刚设置的回环规则。
- `gateway/core/runtime_config.rs::environment_proxy_url_for_target` 在没有环境代理时查询系统代理。
  因回环规则已经丢失，测试请求进入本机系统代理，最终在收到首帧之前报 `Handshake not finished` 或等待超时。

新增 `tests/websocket/proxyEnvironment.rs::LoopbackProxyEnvironment`，统一清理代理别名，
最后发布回环 `NO_PROXY`，析构时明确逆序恢复。逆序恢复同样必要：Vec 默认正序析构会使后创建的
同名别名覆盖最初保存的值。两个独立测试验证正常结束与 panic 时原环境完整恢复，且不打印代理原值。
11 处重复环境初始化全部替换为这一事务；全项目检索未发现剩余的同序设置/清除模式。

18 项官方 Responses WebSocket 本地协议回归全部通过，包括原失败的重连、心跳、连接限制、
初次发送失败、输出后不重放、前导事件后恢复和压缩协商。超时与协议断言保持原样。
两个环境事务测试通过。临时诊断代码全部移除，生产 `responses_websocket.rs` 无改动。
这些是本地协议回归，不计作新的官方模型真实请求。

## 亚毫秒测速计量与运行期 provider 边界（2026-09-08）

继续执行工作区全测时，RPC 集成测试
`rpc_account_proxy_speed_test_uses_custom_proxy_and_updates_latest_only_fields` 暴露另一处计量问题：
固定 10 字节下载可能在 1 毫秒内完成，`download.rs` 先 `as_millis()` 再计算速率，得到 0 毫秒后返回未知。
`upload.rs` 有相同实现。新增 `throughput.rs::measuredMbps`，复用现有高精度吞吐量公式，
成功、失败和取消路径统一传入原始 `Duration`；没有有效字节或真正零耗时仍保持未知。
相关采样实现 `cloudflare_speedtest.rs` 同步移除速率分母的毫秒截断与强制补 1 毫秒，展示字段仍用毫秒。
没有扩大测试下载体或加入延时。59 项测速模块测试及该 RPC 集成用例通过，0.5 毫秒确定性用例验证速率为 0.16 Mbps。

无持久化事件探针同时补齐真实 provider 字段，而非根据模型名称猜测来源：

- 同一匹配 PDB 的 `SessionState::new_with_auto_compact_window_ids` 将 `SessionConfiguration` 复制到 state 偏移 0，大小 `0x328`。
- `SessionConfiguration::thread_config_snapshot`（RVA `0x07437420`）从配置偏移 `0x2f8` 读取原配置 Arc，
  其 provider ID 字符串指针/长度偏移为 `0x2b88`/`0x2b90`。这些字段只适用于此前验证的 Windows x64 构建。
- `runtimeUsageFixture.rs` 在原有 PDB/入口字节门控之后读取这两个有界字段，仅接受 `openai`，
  与客户端文件完成事件采用相同来源边界。未读取命令行、认证材料或无关配置。
- 真实无持久化同进程/thread 的第二轮读到 provider `openai`，模型 `gpt-5.6-sol`，
  输入 31109、缓存 30848、输出 8、总量 31117，与独立 RPC 完成事件核对通过。
  证据目录：`backend/target/observationRuntimeProvider0dbd6520f5b74d0db94c2f5ed2f2d1d0`。

该 provider 读取仍属于测试 DLL 的 ABI 验证，生产完成事件传输、数据库接入与离线恢复尚待实现。

修复后的 `cargo test --manifest-path backend/Cargo.toml --workspace -- --test-threads=1` 完整退出码为 0：
25 个测试/文档测试组累计 2269 项通过、0 项失败、18 项显式跳过；其中服务库 1589 项通过，RPC 集成 49 项通过。
完整输出为 `backend/target/observationWorkspaceFinalTests.log`。新增环境事务的加强断言与真实 provider 探针另行通过，
Tauri 独立工作区检查通过。无持久化探针的子进程、模块目录与公开证书已回收，保留脱敏用量证据。

## 原生完成事件正式接入（2026-09-08）

早期 `runtimeUsageFixture` 和 `runtime` 探针模式已移除，避免维护两套相同偏移。正式链路为：
`directHook/nativeCompletion.rs` → `directCommon/completionSpool.rs` →
`service/directObservation/completionMonitor.rs` → `RecordSink::clientCompleted` → 已有响应去重与费用快照事务。

- 原生入口仍按已验证 PDB/指令字节匹配；检查 `previous_turn_settings` 的 Option niche 后才读取模型。
  不匹配的构建不猜测地址，模型标注为客户端上下文，不伪装成网络返回模型。
- 每次完成只写有界计量元数据，临时副本同步后原子发布；不保存认证、提示词、输出正文或推理内容。
- 模块旁的 `capture.json` 只定位数据目录，实际开关在共享 `observationCompletions/captureControl.json`，
  避免升级后旧模块各自保留过时开关。主动停用在没有运行实例时也能撤销该选择。
- 数据库确认后才删除文件；提交失败、宿主重启或客户端退出都不会依赖进程内游标恢复。
  消费方按响应 ID 去重，原生与网络来源合并，不重复增加请求或费用快照。

真实生产 DLL 验收：

| 场景 | 确认响应 | 输入 / 缓存 / 输出 | 费用快照 / 钱包扣费 |
| --- | ---: | --- | --- |
| 无持久化，同一客户端，正常宿主退出及离线补录 | 3 | 111000 / 104192 / 24 | 3 / 0 |
| 无持久化，强制结束宿主及离线补录 | 3 | 93357 / 92800 / 24 | 3 / 0 |
| 宿主强制退出、客户端也退出，再启动宿主 | 2 | 逐响应与独立 RPC 核对 | 2 / 0 |

对应证据目录：
`observationNativeCompleted50e6808f36648a9a0f2a5287ad98896`、
`observationNativeCrashdeaa6d46d38540cc84354ab053baee43`、
`observationPublisherExit1f7a61d87c824f5889eac6f652257a70`，均位于 `backend/target/`。
无持久化用例断言会话路径为空；离线轮次先确认磁盘事件存在，再让新宿主入库，未以会话文件代替原生入口。

## 实际安装中发现并修复的出口问题

首次第七版安装已移除界面限制，但宿主代理留空时暴露真实网络失败；当时立即关闭观测，恢复原连接路径，
未把该安装视为通过。根因是旧 DLL 对本机 HTTP 代理连接省略私有头，宿主只收到裸 CONNECT，原代理地址丢失。

第八版为所有被改连的连接发送来源头，使用保留位区分直接目标与本机 HTTP 代理。宿主按原代理建立专用连接池，
直接连接固定原 IP，未知协议按原 IP/端口透传。显式代理入口仍使用宿主配置，二者不再混淆。
安装资源改为 `observationHook8.dll`，控制文件改为 `relay8.json`，旧 `hook.json` 保持关闭，避免重新激活旧网络回调。
另修复 `GlobalFree` 包装将 NULL 成功返回误判为错误的诊断，并将每条日志拼成单次追加，避免跨进程分片交错。

宿主代理完全留空、CLI 环境不改写时：

| 协议 | 生成 / 原生确认 | 输入 / 缓存 / 输出 | 费用快照 / 钱包扣费 |
| --- | --- | --- | --- |
| WebSocket | 2 / 2 | 40636 / 19200 / 16 | 2 / 0 |
| SSE | 2 / 2 | 41272 / 22784 / 16 | 2 / 0 |

证据为 `backend/target/observationOriginalProxyf6a14ecd4a32444f997303f6a29ba82a` 和
`backend/target/observationOriginalProxySse71f49e4dcdf04cc0805ba9eca161fed3`。
SSE 的 14 次失败握手仍是测试主动触发的传输回退，不是生成请求。
本地回归另验证宿主配置为另一个错误代理时仍选择客户端原代理，以及 HTTP/TCP 使用原解析地址。

第八版工作区全测：25 组累计 2283 项通过、0 项失败、18 项显式跳过；输出保存在
`backend/target/observationRoute8WorkspaceTests.log`。真实 Winsock 双地址族生命周期、原生元数据与网络双源合并均通过。

## 已安装应用与数据保留

实际目标为 `D:\CodexManager\CodexManager.exe`，不是开发服务器或测试宿主。
旧程序通过正常窗口关闭退出，未强杀或丢弃草稿。替换前使用 SQLite 在线备份合并 WAL，在内存中完成 DPAPI
保护及解密字节校验，再存入原用户数据目录；原数据库不被复制成额外明文文件。
内存恢复检查遵循 [SQLite 的 WAL 反序列化约束](https://www.sqlite.org/c3ref/deserialize.html)，
仅对验证副本调整日志格式头，备份图像本身保持原值。

实际 CLI 沿用原登录与代理，通过已安装应用完成请求 `111`：WebSocket 200，输入 20629、缓存 11392、
输出 9、总量 20638。费用快照按实际单价独立计算为 52151 微美元，该请求的钱包扣费账本条目为 0。
快照的 `charged_cost_microusd` 是计算价格，不等同于已经发生钱包扣款；扣款验证以账本为准。
证据：`backend/target/installedObservation94d39920cd994fd7be30dc2a06a59100/evidence.json`。
启动及开启观测前后的 `auth.json`、`config.toml` 指纹一致，账号 3 个、平台密钥 1 个保持不变。

实际窗口确认日志横幅和切换网关入口消失，仪表盘分析不再模糊遮罩。同步修正接入页与引导页“不记录”的过时文案，
并修复单元格和徽章重复规范化将 `clientResponse` 错标为 HTTP 的问题；客户端事件保留独立类型。

### 最终安装复核

更新最终界面后，再次通过正常关闭/启动重启实际安装程序，不重新点击启用。`relay8.json` 自动恢复为启用，
运行所有者切换到新进程；DPAPI 签名身份指纹保持不变。新的真实 CLI 请求 `127` 成功返回指定文字：
输入 20315、缓存 11392、输出 9、总量 20324，费用快照为 50581 微美元，与快照单价独立计算一致，
该请求没有钱包扣费账本。证据为 `backend/target/installedObservationdbf62bcf35714adcb74bba5c77be81e8/evidence.json`。

实际窗口同时显示“客户端事件”和 WebSocket 200 记录；原横幅、强制切换网关入口及统计模糊遮罩均不再存在。
最后的 `auth.json`、`config.toml`、签名身份及安装文件哈希检查通过，账号和平台密钥数量保持原值。
安装文件与同次 Release 构建一致；最终前端运行期测试 226 项、Tauri 单元测试 76 项通过，桌面 Release 构建通过。

| 目标要求 | 最终证据 |
| --- | --- |
| 原 provider/base_url 和登录身份不被改写 | 独立 CLI 使用原登录；安装、启用、重启后两个客户端文件指纹一致 |
| 不经过账号池、不扣钱包 | 对应记录 `key_id/account_id` 为空；请求 111/127 的扣费账本为 0 |
| 已有与新启动客户端自动观测 | 已有无持久化会话、并发新 CLI、实际已运行客户端均有真实记录 |
| 持久化启用和重启恢复 | 三宿主生命周期、宿主强制退出、发布者退出及实际安装程序重启验证 |
| 请求、用量、费用对应 | 独立 RPC/CLI 记录逐响应核对；正式数据库再按快照单价独立计算 |
| 前端恢复正确展示 | 实际窗口无原限制界面，客户端事件不再误标为 HTTP |
| 构建、清理及回滚 | 全工作区与桌面验证通过；保留程序回滚目录和用户范围 DPAPI 数据库备份 |

当前安装为 `D:\CodexManager\CodexManager.exe` 与 `observationHook8.dll`，观测保持启用。
旧 DLL 仅因已有进程仍持有其映射而保留；旧 `hook.json` 处于关闭状态，不主动卸载其回调。
原始程序回滚点为 `D:\CodexManager\rollback-observation7-20260907T222058Z`，
最终更新前回滚点为 `D:\CodexManager\rollback-observation8-20260907T234930Z`；数据库备份保存在原应用数据目录，
使用 `.db.dpapi` 后缀，仅当前用户可解密。部署与最后核对记录位于 `backend/target/observationDeployment8.json`、
`observationFinalAudit.json`，不含认证原值。
