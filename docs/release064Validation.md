# 0.6.4 交付验证

日期：2026-09-09。

## 自动化

- `pnpm -C frontend run check`：ESLint、115 项运行时测试、静态导出通过。
- `cargo test --offline --manifest-path backend/Cargo.toml --workspace`：1901 项通过、17 项按原测试配置忽略、零失败。
- `cargo test --offline --manifest-path frontend/src-tauri/Cargo.toml --lib`：61 项通过。
- 重置任务覆盖截止边界、重复快照、主次窗口交换、缺失时间、停用账号、跨周期、手动与自然到期合并。
- 旧生命周期测试移除废弃磁盘控制文件断言，保留宿主退出不改变启用选择、主动停用持久化、重复调用幂等的断言。

## 页面与安装

- Playwright 使用隔离 RPC 样本验证 1280×800 和 390×844 会话布局，检查切换账号、重置绑定弹窗及窄屏无水平溢出；没有对真实会话执行切换或删除。
- `pnpm dlx @tauri-apps/cli@2.10.1 build --bundles nsis` 成功。
- 安装包：`D:\Desktop\0\CodexManager\input\SprakCodex-0.6.4-windows-x64-setup.exe`。
- SHA-256：`2FA467AFED4A2FF70F23B66594EE69FF73256F1F4A207089953F51495CE4AC2E`；交付副本与构建产物一致。
- 静默安装退出码 0；`D:\CodexManager\SprakCodex.exe` 产品版本 0.6.4，重启后服务在回环地址 48760 监听。
- 更新前后均为 3 个账号、17 个会话；数据库已登记 4 个未来到期任务。没有人为修改真实额度或消耗重置次数。
- 自然重置后的真实预热尚待上游周期到期；调度去重由隔离数据库测试验证，不能据此声称已观察到真实上游周期重置。

## 回滚

保留 `input/SprakCodex-0.6.3-windows-x64-setup.exe`。新增表不改写账号或会话内容，旧程序不使用该表。回滚时保留应用数据目录。
