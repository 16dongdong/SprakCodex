# 内置注入模块来源与构建

`backend/crates/directHook` 与 `backend/crates/directCommon` 初始导入自
`https://github.com/16dongdong/CProxy`，版本
`21be653e7dc91eb9a4873e1c1627ae4773739ef9`。

这两个目录已成为本仓库源码，不在构建或运行时访问原仓库。
本仓库保留 Relay 字节协议，并维护宿主与 DLL 的共享就绪协议；已移除画像、注册表与子进程控制逻辑。
网络运行期和 trampoline 安装分别位于 `windowsRuntime.rs` 与 `hookInstall.rs`，测试位于独立的 `tests/unit/`。
`relayControl.rs` 管理配置快照，和宿主共用 `directCommon/relayContract.rs`、`runtimeLease.rs`；
有效改连配置必须绑定存活的 Relay 运行线程，就绪事件第三版要求该实例校验。
原始源码许可为 Apache-2.0，完整许可见 `directHookApacheLicense.txt`。

## 构建

Windows 桌面预构建脚本自动执行本项目 Cargo 工作区的
`codexmanager-direct-hook` Release 构建，强制输出到 `backend/target`。
Cargo 根据源码依赖增量编译，失败则中止桌面打包。

`tauri.windows.conf.json` 将 `backend/target/release/cphook.dll`
映射为安装目录中的 `cphook.dll`。不把 DLL 二进制提交到 Git。
其它平台不引用 Windows DLL 资源。

## 验收边界

源码内置和打包成功仅证明产物可构建、可分发。自动进程接管、TLS 信任、
既有连接恢复、协议转发和统计准确性仍需分别通过运行时验收。
