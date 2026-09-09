#TESTING

This document defines SprakCodex repository-level testing and validation baselines.

Target:

- New collaborators can quickly know what at least needs to be run after the change
- Avoid upgrading all changes to full verification
- 让协议兼容、发布链路、设置治理这些高风险改动有固定检查入口

## 1. Basic environment

- Node.js 20
- pnpm 9
- Rust stable
- PowerShell 7+ (Windows Packaging/Script Validation)

## 2. Front-end changes

Scope of application:

- `frontend/src/app/`
- `frontend/src/components/`
- `frontend/src/lib/`
- `frontend/src/hooks/`

Minimal verification:

```bash
pnpm -C frontend run build
pnpm -C frontend run test:runtime
```

illustrate:

- `pnpm -C frontend run build`: Confirm that the Next.js static export link is still normal
- `pnpm -C frontend run test:runtime`: Confirm runtime capability determination and desktop / Web Capability degradation logic has not returned
- If the changes involve runtime identification, Web RPC, desktop/Web difference processing, make up Section 4

## 3. Desktop / Tauri Changes

Scope of application:

- `frontend/src-tauri/`
- Desktop updates, tray, window, command bridge related changes

Minimal verification:

```bash
cargo test --manifest-path backend/Cargo.toml --workspace
```

Additional suggestions:

```powershell
pwsh -NoLogo -NoProfile -File backend/scripts/rebuild.ps1 -Bundle nsis -CleanDist
```

illustrate:

- Whenever you make changes to Tauri bridging or the desktop lifecycle, it's a good idea to do at least one local desktop build verification.

## 4. Web Run shell/deploy compatible changes

Scope of application:

- `backend/crates/web/`
- `frontend/src/lib/api/transport.ts`
- `frontend/src/components/layout/app-bootstrap.tsx`
- `frontend/src/components/layout/header.tsx`
- `frontend/src/components/layout/sidebar.tsx`
- Web Agent, `/api/runtime`, `/api/rpc`, and related changes in deployment methods

Minimal verification:

```bash
pnpm -C frontend run build
pnpm -C frontend run test:runtime
cargo test --manifest-path backend/Cargo.toml -p codexmanager-web
pwsh -NoLogo -NoProfile -File backend/scripts/tests/web_runtime_probe.test.ps1
```

Suggested additions:

```powershell
pwsh -NoLogo -NoProfile -File backend/scripts/tests/web_runtime_probe.ps1 `
  -Base http://localhost:48761
pwsh -NoLogo -NoProfile -File backend/scripts/tests/web_ui_smoke.ps1 -SkipBuild
```

illustrate:

- `pnpm -C frontend run build`: Confirm that front-end static export can still be generated
- `pnpm -C frontend run test:runtime`: Confirm that the front-end runtime contract and capability determination are consistent
- `cargo test --manifest-path backend/Cargo.toml -p codexmanager-web`: Confirm Web Shell Routing and Runtime Probe Contract
- `web_runtime_probe.test.ps1`: Confirm Web script behavior of running shell minimal smoke link
- `web_ui_smoke.ps1`: Confirm key UI behavior of Web page under supported / unsupported running shell

## 5. Rust server-side changes

Scope of application:

- `backend/crates/core/`
- `backend/crates/service/`
- `backend/crates/start/`
- `backend/crates/web/`

Minimal verification:

```bash
cargo test --manifest-path backend/Cargo.toml --workspace
```

Additional suggestions:

```bash
cargo build --manifest-path backend/Cargo.toml -p codexmanager-service --release
cargo build --manifest-path backend/Cargo.toml -p codexmanager-web --release
cargo build --manifest-path backend/Cargo.toml -p codexmanager-start --release
```

## 6. Protocol adaptation/gateway modification

Scope of application:

- `backend/crates/service/src/gateway/`
- `backend/crates/service/src/http/`
- `backend/crates/service/src/lib.rs`

Must cover:

- `/v1/responses`
- `/v1/chat/completions`
- Streaming SSE
- Non-streaming JSON
- `tools`
- `tool_calls`

Minimal verification:

```bash
cargo test --manifest-path backend/Cargo.toml --workspace
pwsh -NoLogo -NoProfile -File backend/scripts/tests/gateway_regression_suite.ps1
pwsh -NoLogo -NoProfile -File backend/scripts/tests/codex_stream_probe.ps1
pwsh -NoLogo -NoProfile -File backend/scripts/tests/chat_tools_hit_probe.ps1
```

illustrate:

- If the local environment does not have a real upstream account, at least run Rust tests and keep probe execution instructions.
- Compatibility fixes cannot validate only one type of client.

## 7. Settings/Environment Variables/Persistence Changes

Scope of application:

- `frontend/src/settings/`
- `backend/crates/service/src/app_settings/`
- `backend/crates/core/src/storage/settings.rs`
- Added `CODEXMANAGER_*` configuration items

Minimal verification:

```bash
pnpm -C frontend run build
cargo test --manifest-path backend/Cargo.toml --workspace
```

Must be manually confirmed:

- Is the default value clear?
- 是否写入 `app_settings`
- Does it need to be synchronized to runtime?
- README / `CONTRIBUTING.md` / `ARCHITECTURE.md` Does it need to be updated?

## 8. Publish link changes

Scope of application:

- `.github/workflows/`
- `.github/actions/`
- `backend/scripts/release/`
- `backend/scripts/rebuild*`

Minimal verification:

```bash
pnpm -C frontend run build
cargo test --manifest-path backend/Cargo.toml --workspace
pwsh -NoLogo -NoProfile -File backend/scripts/tests/assert-release-version.test.ps1
pwsh -NoLogo -NoProfile -File backend/scripts/tests/rebuild.test.ps1
```

Must be manually confirmed:

- The workflow input is consistent with the README description
- There is no drift in product naming
- prerelease / latest behavior without drift

## 9. Document management changes

Scope of application:

- `README*`
- `ARCHITECTURE.md`
- `CONTRIBUTING.md`
- `CHANGELOG.md`
- `docs/`

Minimal verification:

- Check if the link path is valid
- Check if document responsibilities are duplicated
- Check whether the version number, product name, and workflow name are consistent with the actual ones

## 10. Minimum checking recommendations before submission

### General changes

```bash
pnpm -C frontend run build
pnpm -C frontend run test:runtime
cargo test --manifest-path backend/Cargo.toml -p codexmanager-web
```

### Front-end page changes

```bash
pnpm -C frontend run build
pnpm -C frontend run test:runtime
```

### Web Compatibility/Deployment Changes

```bash
pnpm -C frontend run build
pnpm -C frontend run test:runtime
cargo test --manifest-path backend/Cargo.toml -p codexmanager-web
pwsh -NoLogo -NoProfile -File backend/scripts/tests/web_runtime_probe.test.ps1
```

### Protocol adaptation changes

```bash
cargo test --manifest-path backend/Cargo.toml --workspace
pwsh -NoLogo -NoProfile -File backend/scripts/tests/gateway_regression_suite.ps1
```

## 11. Result recording convention

- Verifications that can be completely executed are recorded as "executed".
- Verifications that cannot be executed due to environmental restrictions are clearly written as "not executed + reason".
- Don't take "should be able to pass" as "proven".