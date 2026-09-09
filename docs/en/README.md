# SprakCodex

## Home Navigation

| What you want to do | Go directly to |
| --- | --- |
| First launch, deployment, Docker, or macOS allowlisting | [Runtime and Deployment Guide](report/runtime-and-deployment-guide.md) |
| Configure Codex CLI / ccswitch, `auth.json`, and `config.toml` | [Runtime and Deployment Guide](report/runtime-and-deployment-guide.md#connect-through-ccswitch) |
| Import an account from ChatGPT `/api/auth/session` without logging in through Codex | [Chinese guide: Import ChatGPT auth session](../zh-CN/report/不登陆Codex使用ChatGPT-auth-session导入账号.md) |
| Configure ports, proxy, database, Web password, and environment variables | [Environment and Runtime Configuration](report/environment-and-runtime-config.md) |
| Troubleshoot account routing, import failures, challenge interception, or request errors | [FAQ and Account Routing Rules](report/faq-and-account-routing-rules.md) |
| Understand why background jobs skip, disable, or deactivate accounts | [Background Task Account Skip Notes](report/background-task-account-skip-notes.md) |
| Manage models, pricing, routes, instructions policy, and local cache exports | [Chinese guide: Model Catalog V2 Management and Billing](../zh-CN/report/模型目录V2管理与计费说明.md) |
| Minimal plugin center integration and quick onboarding | [Plugin Center Minimal Integration](report/plugin-center-minimal-integration.md) |
| Integrate with the plugin center, view interfaces, marketplace modes, and Rhai APIs | [Plugin Center Integration and Interfaces](report/plugin-center-integration-and-interfaces.md) |
| View every internal integration interface | [System Internal Interface Inventory](report/system-internal-interface-inventory.md) |
| Build, package, release, or call scripts locally | [Build, Release, and Script Guide](release/build-release-and-scripts.md) |

## Feature Overview

- Account pool management: groups, tags, ordering, notes, ban recognition, and ban filtering.
- Batch import/export: multi-file import, recursive JSON folder import on desktop, and per-account single-file export.
- Usage display: standard 5-hour + 7-day windows, 7-day-only accounts, and official additional buckets such as Code Review / Spark; refresh shows remaining percentages and reset times consistently.
- Account authorization: `chatgpt.com` browser OAuth and Device Code login; browser OAuth also supports manually pasting the callback URL.
- Platform keys: random or custom fixed keys, disabling, deletion, model binding, reasoning tier, and service tier (follow request / Standard / Fast / Ultrafast / Flex). Keys can be bound to custom account groups and intersected with plan filters so rotation stays inside the authorized pool.
- Model management: Model Catalog V2 is the sole runtime source of truth. It supports builtin/custom models, integer three-tier and long-context pricing, account-pool and aggregate-API routes, instructions policy, local JSON preview/commit, and proactive Codex cache export from desktop and Web.
- Aggregate API: manage minimal third-party upstreams, including create, edit, balance, and connectivity tests against configured V2 routes. It does not auto-discover provider models; administrators fetch and selectively link models to Catalog V2.
- Plugin center: `/plugins/` supports built-in curated, enterprise private, and custom source marketplace modes, plus manifests, tasks, logs, and Rhai interfaces.
- Skills and plugins: `/skills/` separates Skills Installation from Codex Plugin Installation. It supports GitHub repositories, skills.sh search, ZIP/directory import, installed-item management, and the native Marketplace plugin flow; `.system` Skills remain read-only.
- Desktop project launcher: bookmark local directories; Windows and macOS open them in the ChatGPT Codex App, while Sessions opens the `resume` selector in a new terminal with the local SprakCodex profile.
- Settings: includes system-derived settings, per-account concurrency, upstream proxy, total and stream-idle timeouts, SSE keepalive, and conservative high-concurrency degradation. Disable keepalive with `CODEXMANAGER_SSE_KEEPALIVE_ENABLED=0`; enable experimental upstream WebSocket with `CODEXMANAGER_USE_WEBSOCKET_UPSTREAM=1`.
- System internal interface inventory: all desktop/service commands, RPC methods, and built-in plugin functions.
- Local service: automatic startup with configurable port and listen address.
- Local gateway: one OpenAI-compatible endpoint for Codex CLI, Gemini CLI, Claude Code, and third-party tools; supports Gemini to `/v1/responses`, SSE, tools, MCP, skills, and request/stream timeouts.
- Image generation: injects the official Codex `image_generation` tool for `/v1/responses` by default and provides `/v1/images/generations` and `/v1/images/edits`; the default model is `gpt-image-2`.

## Screenshots

![Dashboard](../assets/images/dashboard.png)
![Account Management](../assets/images/accounts.png)
![Platform Key](../assets/images/platform-key.png)
![Aggregate API](../assets/images/aggregate-api.png)
![Plugin Center](../assets/images/plug.png)
![Log View](../assets/images/log.png)
![Settings](../assets/images/themes.png)

## Quick Start

1. Launch the desktop app and click **Start Service**.
2. Open **Account Management** and choose browser authorization or Device Code login for `chatgpt.com`.
3. If the browser callback fails, paste the callback URL to complete parsing manually.
4. Refresh usage and verify account status.

## Default Data Directory

- The desktop app stores its SQLite database in the application data directory as `codexmanager.db`.
- Windows: `%APPDATA%\\com.codexmanager.desktop\\codexmanager.db`
- macOS: `~/Library/Application Support/com.codexmanager.desktop/codexmanager.db`
- Linux: `~/.local/share/com.codexmanager.desktop/codexmanager.db`
- Before first initialization after an upgrade, a `codexmanager.db.pre-<version>.bak` snapshot is retained in the same directory; retries for the same version do not overwrite it.
- **Desktop Diagnostics** can enable Debug mode, disable ordinary file logs, and open the log directory. Runtime logs are capped at 512 KB; request logs and token/cost statistics are unaffected.
- If the UI cannot start, use `CodexManager.exe --debug` (`CodexManager --debug` on macOS/Linux). Startup failures display the reason and write `startup-error.log` to the shown log directory.
- For database, proxy, listen-address, and other settings, see [Environment and Runtime Configuration](report/environment-and-runtime-config.md).
- Docker defaults to `TZ=Asia/Shanghai`; Compose inherits the deployment `TZ` or falls back to `Asia/Shanghai`. Use the appropriate IANA time zone elsewhere.

## Product Views

### Desktop

- Account Management: import, export, refresh accounts and usage, with low-quota/ban filters and reset times.
- Platform Key: bind keys by model, reasoning tier, and service tier, and inspect request logs.
- Model Management: desktop and Web do not write or download `~/.codex/models_cache.json`; direct accounts follow the official catalog, while the local gateway uses an independent managed catalog.
- Plugin Center: `/plugins/` provides marketplace switching, installation, enable/disable, tasks, logs, and Rhai integration.
- Skills and Plugins: `/skills/` separates Skills and Codex plugins, with repository/skills.sh install, ZIP/directory import, safe uninstall, native Marketplace flow, and read-only system Skills.
- Project Launcher: desktop bookmarks local folders; Windows/macOS open them in the ChatGPT Codex App, Sessions uses the local CLI, and Web/Docker do not access device directories.
- Settings: manage port, listen address, proxy, timeouts, SSE keepalive, theme, updates, and background behavior.

### Service Edition

- `codexmanager-service`: local OpenAI-compatible gateway.
- `codexmanager-web`: browser management page plus `/api/runtime` and `/api/rpc` proxies.
- `codexmanager-start`: starts service + web together.

## Common Documentation

- Version history: [CHANGELOG.md](CHANGELOG.md)
- Contribution guidelines: [CONTRIBUTING.md](CONTRIBUTING.md)
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- Testing baseline: [TESTING.md](TESTING.md)
- Security: [SECURITY.md](SECURITY.md)
- Model Catalog V2: [Chinese guide](../zh-CN/report/模型目录V2管理与计费说明.md)
- Documentation index: [Chinese documentation index](../zh-CN/README.md)

## Topic Pages

| Page | Contents |
| --- | --- |
| [Runtime and Deployment Guide](report/runtime-and-deployment-guide.md) | First launch, Docker, Service edition, and macOS allowlisting |
| [Environment and Runtime Configuration](report/environment-and-runtime-config.md) | Application settings, proxy, listen address, database, and Web security |
| [FAQ and Account Routing Rules](report/faq-and-account-routing-rules.md) | Account routing, challenge interception, import/export, and common errors |
| [Chinese guide: Import ChatGPT auth session](../zh-CN/report/不登陆Codex使用ChatGPT-auth-session导入账号.md) | Copy ChatGPT session JSON and batch-import it into the account pool |
| [Background Task Account Skip Notes](report/background-task-account-skip-notes.md) | Background filtering, disabled accounts, and workspace deactivation |
| [Minimal Troubleshooting Guide](report/minimal-troubleshooting-guide.md) | Service startup, request forwarding, and model refresh failures |
| [Plugin Center Integration and Interfaces](report/plugin-center-integration-and-interfaces.md) | Routes, marketplace modes, Tauri/RPC interfaces, manifest fields, and Rhai functions |
| [Build, Release, and Script Guide](release/build-release-and-scripts.md) | Local builds, Tauri packaging, workflows, and script parameters |
| [Release and Artifacts](release/release-and-artifacts.md) | Platform artifacts, naming, and pre-release behavior |
| [Script and Release Responsibility Matrix](report/script-and-release-responsibility-matrix.md) | Script responsibilities and usage scenarios |
| [Gateway vs Codex Headers and Params](report/gateway-vs-codex-headers-and-params.md) | Gateway forwarding compared with Codex headers and parameters |
| [System Internal Interface Inventory](report/system-internal-interface-inventory.md) | Desktop, service, and plugin-center interfaces |
| [CHANGELOG.md](CHANGELOG.md) | Releases, unreleased changes, and full history |

## Directory Structure

```text
.
├─ frontend/                # Frontend and Tauri desktop app
│  ├─ src/
│  ├─ src-tauri/
│  └─ out/
├─ backend/crates/              # Rust core/service
│  ├─ core
│  ├─ service
│  ├─ start              # Starts service + web
│  └─ web                # Service Web UI and /api/rpc proxy
├─ docs/                # Official documentation
├─ backend/scripts/             # Build and release scripts
└─ README.md
```

## Acknowledgements and Reference Projects

- Codex (OpenAI): referenced for request flows, login semantics, and upstream-compatible source structure <https://github.com/openai/codex>
- CLIProxyAPI (CPA): referenced for Responses request conversion and tool-call conventions <https://github.com/router-for-me/CLIProxyAPI>
