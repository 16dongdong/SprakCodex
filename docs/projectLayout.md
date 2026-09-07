# 前后端目录与运行边界

## 目录职责

```text
CodexManager/
├─ frontend/
│  ├─ src/              # Next.js 页面、组件、状态和类型化 API 客户端
│  ├─ src-tauri/        # Tauri 桌面壳、原生命令与窗口生命周期
│  ├─ tests/            # 前端运行时、目录契约和浏览器回归
│  └─ package.json      # 前端依赖与构建入口
├─ backend/
│  ├─ crates/           # core、rusqlite、service、web、start
│  ├─ scripts/          # 本地构建、发布与服务启动脚本
│  ├─ docker/           # Dockerfile、Compose 和反向代理配置
│  ├─ .cargo/           # 后端编译配置
│  └─ Cargo.toml        # 独立 Rust 工作区
└─ docs/
   ├─ assets/           # 文档图片及分发辅助资源
   ├─ skills/           # 客户端调用示例
   └─ reports/          # 设计与问题分析记录
```

根目录只保留仓库治理文件及 `.github/` 等版本控制配置。`node_modules`、`target`、`.next`、`out` 均是本地生成目录，不提交到 Git。

## 接口边界

- 前端所有业务请求继续通过 `frontend/src/lib/api/` 的类型化客户端。
- 浏览器模式：Next 开发服务将 `/api/runtime`、`/api/rpc` 和事件接口转发到 Web 接入层；该层再与 service 通信。无需把业务代码放回前端，也不新增跨域放行。
- 桌面模式：Tauri 命令调用 `backend/crates/` 中的服务实现。桌面壳留在前端工程，便于 Tauri CLI、静态导出与窗口生命周期共同构建。
- Web 发布模式：先生成 `frontend/out`，再由 `backend/crates/web/build.rs` 嵌入静态资源；也可设置 `CODEXMANAGER_WEB_ROOT` 使用独立静态目录。
- 数据库结构、RPC 名称、端口与用户数据目录没有变化。

## 本地开发

以下命令从仓库根目录执行，三个服务分别运行在独立终端：

```powershell
# 终端一：业务服务
cargo run --manifest-path backend/Cargo.toml -p codexmanager-service

# 终端二：Web 接入层
$env:CODEXMANAGER_WEB_NO_SPAWN_SERVICE = "1"
$env:CODEXMANAGER_WEB_NO_OPEN = "1"
cargo run --manifest-path backend/Cargo.toml -p codexmanager-web

# 终端三：前端开发服务，默认访问 http://localhost:3000
pnpm -C frontend install --frozen-lockfile
pnpm -C frontend run dev
```

Web 接入层不在默认地址时，启动前端前设置 `CODEXMANAGER_DEV_WEB_ORIGIN`。它只用于开发代理，不改变生产 RPC 契约。

## 验证与打包

```powershell
pnpm -C frontend run test:runtime
pnpm -C frontend run build:desktop
pnpm -C frontend run test:navigation
cargo check --manifest-path backend/Cargo.toml --workspace --all-targets --locked
cargo test --manifest-path backend/Cargo.toml -p codexmanager-web --locked
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --test app_settings --locked
```

桌面壳不属于后端 Cargo 工作区，单独验证与构建：

```powershell
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib
pwsh -File backend/scripts/rebuild.ps1 -Bundle nsis
```

Docker 源码构建必须以仓库根目录作为上下文，才能同时读取前后端工程：

```powershell
docker compose -f backend/docker/docker-compose.yml config
docker compose -f backend/docker/docker-compose.yml build
docker build -f backend/docker/Dockerfile.all-in-one .
```

迁移前已经存在的全量 WebSocket 回归失败不属于目录迁移的通过依据；应分别记录编译、前端契约、Web 接入层和完整回归结果。
