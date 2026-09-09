# 内置注入模块来源与构建

`backend/crates/directHook` 与 `backend/crates/directCommon` 初始导入自
`https://github.com/16dongdong/CProxy`，版本
`21be653e7dc91eb9a4873e1c1627ae4773739ef9`。

这两个目录已成为本仓库源码，不在构建或运行时访问原仓库。
本仓库保留 Relay 字节协议，并维护宿主与内存映像的共享就绪协议；已移除画像、注册表与子进程控制逻辑。
网络运行期和 trampoline 安装分别位于 `windowsRuntime.rs` 与 `hookInstall.rs`，测试位于独立的 `tests/unit/`。
`relayControl.rs` 管理配置快照，和宿主共用 `directCommon/relayContract.rs`、`runtimeLease.rs`；
有效改连配置必须绑定存活的 Relay 运行线程。`trustProvider.rs` 与 `trustBundle.rs` 只接入额外 CA 读取，
合并公开证书并保留原登录与环境块。`proxyDiscovery.rs` 和 `systemProxy.rs` 只读目标代理元数据，
代替手工端口列表。`runtimeMetadata.rs` 通过共用的 `runtimeHome.rs` 发布公开运行目录，
就绪事件第十一版同时要求运行实例、CA 读取、代理发现与目录元数据发布完成。
原始源码许可为 Apache-2.0，完整许可见 `directHookApacheLicense.txt`。

## 构建

Windows 构建 `codexmanager-service` 时，其 `build.rs` 自动执行本项目 Cargo 工作区的
`codexmanager-direct-hook` Release 构建，并将产物复制到当前 `OUT_DIR`。运行代码通过
`include_bytes!` 把该 PE 字节链接进宿主 EXE；Tauri 资源清单不再包含观测 DLL，二进制也不提交到 Git。
其它平台不构建或链接 Windows 载荷。

## 验收边界

源码内置和打包成功仅证明产物可构建、可链接。内存映射、自动进程接入、TLS 信任、
既有连接恢复、协议转发和统计准确性仍需分别通过运行时验收。
