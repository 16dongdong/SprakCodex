#ARCHITECTURE

This document describes SprakCodex the current repository structure, running relationships, and release links. The goal is to help collaborators quickly determine which layer the changes should fall on.

## 1. Overall shape

SprakCodex consists of two types of operating modes:

1. Desktop mode: Tauri Desktop + local service process
2. Service Mode: Standalone service + web UI, can be used with server, Docker or no desktop environment

Unified goal:

- Manage accounts, usage, and platform keys
- Provide local gateway capabilities
- Externally compatible with OpenAI style entry, and adapted to multiple upstream protocols

## 2. Directory structure and responsibilities

```text
.
├─ frontend/              # Next.js 前端与 Tauri 桌面壳
│  ├─ src/                # 页面、组件、状态与 API 客户端
│  ├─ src-tauri/          # 桌面原生命令和打包配置
│  └─ tests/              # 前端回归
├─ backend/               # 独立 Rust 工作区
│  ├─ crates/             # core、rusqlite、service、web、start
│  ├─ scripts/            # 构建和发行脚本
│  └─ docker/             # 容器部署与代理配置
├─ docs/                  # 文档、assets 图片与 skills 示例
└─ .github/               # CI 与发布流程
```

## 3. Core complex domain entry index

### 3.1 Front-end master control entrance

- `frontend/src/main.js`: Front-end startup assembly entrance
- `frontend/src/runtime/app-bootstrap.js`: Interface initialization arrangement
- `frontend/src/runtime/app-runtime.js`: Coordination of refresh process and runtime
- `frontend/src/settings/controller.js`: Set up domain facade and continue distribution to submodules

### 3.2 Desktop shell entrance

- `frontend/src-tauri/src/lib.rs`: Tauri Application assembly entry
- `frontend/src-tauri/src/settings_commands.rs`: Desktop setting bridge command
- `frontend/src-tauri/src/service_runtime.rs`: Desktop embedded service life cycle
- `frontend/src-tauri/src/rpc_client.rs`: Desktop RPC Call infrastructure

### 3.3 service gateway and protocol entry

- `backend/crates/service/src/lib.rs`: Service main entrance and runtime assembly
- `backend/crates/service/src/http/`: HTTP routing entry
- `backend/crates/service/src/rpc_dispatch/`: RPC Distribution entrance
- `backend/crates/service/src/gateway/mod.rs`: Gateway aggregation entry
- `backend/crates/service/src/gateway/observability/http_bridge.rs`: Request tracking, protocol bridging, log writing
- `backend/crates/service/src/gateway/protocol_adapter/request_mapping.rs`: OpenAI/Codex input mapping
- `backend/crates/service/src/gateway/protocol_adapter/response_conversion.rs`: Non-streaming result total conversion entry
- `backend/crates/service/src/gateway/protocol_adapter/response_conversion/sse_conversion.rs`: Streaming SSE Conversion Entry
- `backend/crates/service/src/gateway/protocol_adapter/response_conversion/openai_chat.rs`: OpenAI Chat result adaptation
- `backend/crates/service/src/gateway/protocol_adapter/response_conversion/tool_mapping.rs`: Tool name shortening and restoration

### 3.4 Setup and run configuration entry

- `backend/crates/service/src/app_settings/`: Set up persistence, environment variable coverage, runtime synchronization
- `backend/crates/service/src/web_access.rs`: Web Access password and session token

## 4. Running relationship

### 4.1 Desktop mode

Desktop mode consists of the following parts:

- `frontend/src/`: Front-end UI
- `frontend/src-tauri/`: Desktop shell
- `backend/crates/service/`: local service

How to run:

1. The user launches the desktop application.
2. Tauri The shell is responsible for desktop behaviors such as windows, trays, updates, single instances, and setting bridges.
3. The desktop communicates with `codexmanager-service` via RPC or a local address.
4. The front-end UI displays pages such as account, usage, request log, and settings.

### 4.2 Service Mode

The Service pattern consists of the following binaries:

- `codexmanager-service`
- `codexmanager-web`
- `codexmanager-start`

Responsibilities:

- `codexmanager-service`: Core service process, providing account management, gateway forwarding, request logs, setting persistence, and RPC/HTTP interfaces.
- `codexmanager-web`: Web UI service shell, which can directly provide front-end pages and proxy to local services.
- `codexmanager-start`: A one-click launcher for publishing packages, responsible for launching service and web at the same time.

## 5. Module responsibilities

### 5.1 `frontend/src/`

Mainly responsible for:

- Page rendering
- user interaction
- Status management
- Call local API / Tauri command
- Front-end logic of settings page and account page

### 5.2 `frontend/src-tauri/`

Mainly responsible for:

- Tauri Application startup
- Single instance control
- System tray and window events
- Desktop updates and installer behavior
- Bridge front-end operations to service/local runtime

### 5.3 `backend/crates/core/`

Mainly responsible for:

- SQLite Migration
- Storage underlying capabilities
- Core basic logic such as authentication/usage
- Data access capabilities that can be reused by services

### 5.4 `backend/crates/service/`

Mainly responsible for:

- HTTP / RPC Portal
- Account, usage, API Key management
- Local gateway capabilities
- Protocol adaptation and upstream forwarding
- Request logging and setting persistence
- Runtime configuration synchronization

Key subdirectories:

- `src/gateway/`: Gateway, protocol adaptation, streaming and non-streaming conversion
- `src/http/`: HTTP routing entry
- `src/rpc_dispatch/`: RPC Distribution
- `src/account/`, `src/apikey/`, `src/requestlog/`, `src/usage/`: Domain logic

### 5.5 `backend/crates/web/`

Mainly responsible for:

- Provide Web UI static resources
- Mount or proxy to service
- Optionally embed `frontend/dist` into the binary to form a single-file distribution

### 5.6 `backend/crates/start/`

Mainly responsible for:

- Provide a more direct startup entry in the Service release package
- Coordinate the life cycle of service and web

## 6. Data and configuration

### 6.1 Database

The current project uses SQLite.
Database migration is located at:

- `backend/crates/core/migrations/`

The database not only stores accounts, but also assumes:

- API Key
- Request log
- token statistics
- app settings

### 6.2 Run configuration

The main sources of configuration include:

- Environment variables `CODEXMANAGER_*`
- `.env` / `codexmanager.env` in the application running directory
- `app_settings` Persistence table
- Desktop settings page

Current agreement:

- Configurations that must take effect before startup are retained at the environment variable layer.
- Runtime tunable configurations are first managed through the settings page + `app_settings`.
- Setting changes should not be scattered across desktops, frontends, and services without boundaries.

## 7. Request link overview

Typical request links are as follows:

1. The client or UI initiates the request.
2. Requests enter the HTTP/RPC layer of `backend/crates/service`.
3. The gateway module determines the forwarding strategy, account number, header strategy, upstream proxy, etc.
4. The protocol adaptation layer is responsible for processing:
   - `/v1/chat/completions`
   - `/v1/responses`
   - Streaming SSE
   - Non-streaming JSON
   - `tool_calls` / tools mapping and aggregation
5. The results are written back to the request log and statistics, and then returned to the caller.

## 8. Build and publish links

### 8.1 Local development and build

front end:

- `pnpm -C frontend run dev`
- `pnpm -C frontend run build`
- `pnpm -C frontend run check`

Rust:

- `cargo test --manifest-path backend/Cargo.toml --workspace`
- `cargo build --manifest-path backend/Cargo.toml -p codexmanager-service --release`
- `cargo build --manifest-path backend/Cargo.toml -p codexmanager-web --release`
- `cargo build --manifest-path backend/Cargo.toml -p codexmanager-start --release`

Desktop:

- `backend/scripts/rebuild.ps1`
- `backend/scripts/rebuild-linux.sh`
- `backend/scripts/rebuild-macos.sh`

### 8.2 Version Management

The version is currently maintained uniformly by the root workspace:

- Root `backend/Cargo.toml` of `[workspace.package].version`

Additional synchronization on desktop:

- `frontend/src-tauri/Cargo.toml`
- `frontend/src-tauri/tauri.conf.json`

Unified modification entry:

- `backend/scripts/bump-version.ps1`

### 8.3 GitHub Release

Main publishing entrance:

- `.github/workflows/release-all.yml`

Responsibilities:

- Build Windows / macOS / Linux Desktop Product
- Build version Service artifact
- Upload GitHub Release attachment
- Determine release type based on tag / `prerelease` input

## 9. Current structural risks

The current repository needs to focus on the following issues:

1. `frontend/src-tauri/src/lib.rs` It is still thick, and the desktop shell assembly and command implementation still need to be disassembled.
2. `backend/crates/service/src/lib.rs` Configuration, runtime synchronization, and side effect boundaries are not clear enough.
3. `backend/crates/service/src/gateway/protocol_adapter/response_conversion.rs` There are many compatible branches and the risk of regression is high.
4. `.github/workflows/release-all.yml` Still long, multi-platform logic requires persistence constraints.

## 10. Suggested changes

In order to reduce structural pollution, new demands should be targeted according to the following principles:

- New pages or front-end interactions: Priority falls in `frontend/src/views/`, `frontend/src/services/`, `frontend/src/ui/`
- New Desktop Capabilities: Prioritize standalone modules that fall into `frontend/src-tauri/src/`, rather than continuing to cram them all into `lib.rs`
- New setting item: first determine whether it belongs to environment variables, persistent configuration or runtime state
- Compatible with new protocols: priority should be placed in the gateway / protocol adapter submodule, and do not continue to stack conditional branches out of order.
- New release logic: Give priority to drawing scripts or reusing steps, and do not repeat modifications three times on three platforms.