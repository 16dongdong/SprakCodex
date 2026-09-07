# Comparison of scripts and publishing responsibilities

## Comparison table between `.github/actions/` and `scripts/release/`

| Scenario | GitHub Action | Corresponding script/responsibility |
|---|---|---|
| Tauri Build retry | `build-tauri-with-retry` | `scripts/release/build-tauri-with-retry.ps1` / `.sh` |
| Service Packaging | `stage-service-package` | `scripts/release/stage-service-package.ps1` / `.sh` |
| GitHub Release | `publish-github-release` | `scripts/release/publish-github-release.sh` |
| Release environment preparation | `setup-release-env` | Workflow internal environment assembly, currently there is no completely equivalent independent top-level script |
| Front-end dist preparation | `prepare-frontend-dist` | Build/download front-end product endpoint within workflow, currently there is no separate top-level script |

## Boundary Agreement

### `.github/actions/`

Responsible for:

- Reusable steps within workflow
- Unify input and output across jobs
- CI/Release environment encapsulation

### `scripts/release/`

Responsible for:

- Actual building, packaging and publishing actions
- Script implementation called locally or by workflow
- Try to keep it independently executable

### `scripts/*.ps1|*.sh`

Responsible for:

- Top-level entrance for developers
- Unified parameter organization
- Close the complex release sub-steps

## Current historical script inventory

### Still valuable

- `rebuild.ps1`
- `bump-version.ps1`
- `tests/gateway_regression_suite.ps1`
- `tests/chat_tools_hit_probe.ps1`
- `tests/codex_stream_probe.ps1`
- `release/*`

Reason:

- Still actually used by local development, release links or workflows
- Not orphaned legacy files

### There are currently no scripts found that have "only a single point of use and can be deleted directly"

The more appropriate strategy at this stage is not deletion, but:

1. Use `scripts/README.md` to indicate entrance stratification
2. Describe CI-specific scripts and local entries separately
3. If a script is not referenced in multiple versions in a row, consider archiving or merging it.

## Maintenance recommendations

- When adding workflow capabilities, give priority to determining whether existing actions should be reused.
- When adding a new release script, first decide whether it is a "top-level entry" or a "CI sub-step"
- If the script is not suitable for local execution, it must be clearly marked as "CI only" in the README