# 后端工程

本目录是独立 Rust 工作区。账号管理、网关、存储与 Web 接入均位于 `crates/`；部署文件位于 `docker/`，构建脚本位于 `scripts/`。

在本目录执行：

```powershell
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo run -p codexmanager-service
```

前端独立开发时，另开终端启动 Web 接入层：

```powershell
$env:CODEXMANAGER_WEB_NO_SPAWN_SERVICE = "1"
$env:CODEXMANAGER_WEB_NO_OPEN = "1"
cargo run -p codexmanager-web
```

默认 service 地址为 `localhost:48760`，Web 接入地址为 `localhost:48761`。配置仍使用现有 `CODEXMANAGER_*` 环境变量。

Web 发行版通过 `embedded-ui` 功能嵌入 `../frontend/out`。前端源码不参与 Rust 编译；打包前先执行 `pnpm -C ../frontend run build:desktop`。

目录边界、桌面启动和 Docker 命令见 [项目布局](../docs/projectLayout.md)。
