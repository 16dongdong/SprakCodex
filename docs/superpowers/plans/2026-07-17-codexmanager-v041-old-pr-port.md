# CodexManager v0.4.1 旧 PR 移植实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 PR #346 的九个修复安全合入官方当前 `main`，构建并安装 arm64 CodexManager，并用真实 RPC 与 Responses 请求确认 404、profile 判定和图像工具冲突已解决。

**Architecture:** 使用普通 merge 保留旧 PR 历史和官方新版历史。`git merge-tree` 已证明当前两端没有文本冲突，因此不计划重写功能文件；合并后用旧 PR 的回归测试逐项证明修复仍然生效，再运行新版测试保护 Gemini、compact、模型目录 v2 和桌面更新代码。所有检查通过后才构建、安装和更新 PR。

**Tech Stack:** Rust、Cargo、Tauri 2、Next.js 16、pnpm、SQLite、macOS codesign、Git、GitHub CLI、JSON-RPC、OpenAI Responses API。

---

## 文件职责

合并后需要重点检查的文件如下：

- `crates/service/src/codex_profile.rs`：读取 Codex profile，判断网关模式并返回状态。
- `crates/service/src/codex_profile_tests.rs`：覆盖 experimental 配置、旧 marker 和登录信息并存的情况。
- `crates/service/src/gateway/request/incoming_headers.rs`：读取客户端传入的 `session-id`、`thread-id` 等请求头。
- `crates/service/src/gateway/request/official_responses_http.rs`：处理 Responses 请求体和图像工具自动加入。
- `crates/service/src/gateway/request/tests/incoming_headers_tests.rs`：覆盖连字符请求头。
- `crates/service/src/gateway/request/tests/request_rewrite_tests.rs`：覆盖图像工具、compact 和 Responses 请求体。
- `crates/service/src/gateway/upstream/headers/codex_headers.rs`：生成发往 Codex 上游的请求头。
- `crates/service/src/gateway/upstream/attempt_flow/transport.rs`：发送上游请求，提供无会话头发送方式，并保留 Gemini/CPA 请求头规则。
- `crates/service/src/gateway/upstream/attempt_flow/postprocess.rs`：处理官方 Responses 400 的同地址重试。
- `crates/service/src/gateway/upstream/support/retry.rs`：阻止 Responses 请求访问旧 Codex v1 地址。
- `crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor.rs`：新版候选请求和现有 400 重试，必须保持不变。
- `apps/src-tauri/Info.plist`：声明 `LSRequiresCarbon=false`。
- `apps/src-tauri/tauri.conf.json`：保持 v0.4.1 桌面包配置。
- `docs/superpowers/specs/2026-07-17-codexmanager-v041-old-pr-port-design.md`：已经批准的设计说明。

当前合并预演树为 `dea77c845e91a6e9c1f95a8e82482d148570e96e`。它相对 `origin/main` 只包含旧 PR 修复、旧 Responses 文档和本次设计说明，没有发现额外功能改动。

### Task 1: 合入官方 main

**Files:**
- Merge: `origin/main` into `agent/local-combined-fixes`
- Verify: repository-wide merge result

- [ ] **Step 1: 确认工作区干净且分支正确**

Run:

```bash
git status -sb
git branch --show-current
```

Expected:

```text
## agent/local-combined-fixes...fork/agent/local-combined-fixes [ahead 2]
agent/local-combined-fixes
```

`ahead 2` 对应设计说明和本实施计划。若出现其他未提交文件，停止合并并先确认来源。

- [ ] **Step 2: 获取官方最新 main 并再次预演合并**

Run:

```bash
git fetch origin main
git merge-tree --write-tree HEAD origin/main
```

Expected: 输出一个合并树哈希，命令退出码为 0，不出现 `CONFLICT`。

- [ ] **Step 3: 创建普通 merge commit**

Run:

```bash
git merge --no-ff origin/main -m "merge: 合入官方 main 并保留旧 PR 修复"
```

Expected: Git 创建一个含两个父提交的 merge commit，不需要手工处理文件。

- [ ] **Step 4: 检查 merge commit 和版本**

Run:

```bash
git show --no-patch --format='%H%n%P%n%s' HEAD
rg -n '"version": "0.4.1"' apps/src-tauri/tauri.conf.json
rg -n '^version = "0.4.1"' apps/src-tauri/Cargo.toml
```

Expected:

- merge commit 有两个父提交。
- Tauri 配置和 Tauri crate 都是 `0.4.1`。
- 不创建额外提交；本步骤的 merge commit 就是该任务的提交。

### Task 2: 检查合并后的修复范围

**Files:**
- Verify: all files listed in “文件职责”
- Verify: `docs/superpowers/plans/2026-07-12-codex-responses-400-retry.md`
- Verify: `docs/superpowers/specs/2026-07-12-codex-responses-400-retry-design.md`

- [ ] **Step 1: 检查相对官方 main 的文件列表**

Run:

```bash
git diff --check origin/main...HEAD
git diff --name-status origin/main...HEAD
```

Expected:

- `git diff --check` 没有输出。
- 差异仅包含九个旧修复涉及的 15 个 Rust 代码与测试文件、`apps/src-tauri/Info.plist`、两份旧 Responses 文档、本次设计说明和本实施计划。

- [ ] **Step 2: 确认新版关键代码仍存在**

Run:

```bash
rg -n 'gemini_native_does_not_forward_thread_anchor_as_prompt_cache_key' crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor_tests.rs
rg -n 'responses_maps_client_ultra_to_upstream_max' crates/service/src/gateway/request/tests/request_rewrite_tests.rs
rg -n 'gpt-5\.6|GPT-5\.6' crates apps | head -20
rg -n 'select_macos_dmg_asset_for_arch' apps/src-tauri/src/commands/updater/prepare.rs
```

Expected: 四类新版代码都能找到。

- [ ] **Step 3: 确认旧修复的关键代码形态**

Run:

```bash
rg -n 'struct DetectedGatewayConfig|fn detect_gateway_config|fn gateway_base_urls_match' crates/service/src/codex_profile.rs
rg -n 'send_upstream_request_without_session_headers|is_session_scoped_header' crates/service/src/gateway/upstream/attempt_flow/transport.rs
rg -n 'retry_chatgpt_responses_bad_request_without_session_headers' crates/service/src/gateway/upstream/attempt_flow/postprocess.rs
rg -n 'should_skip_codex_v1_alt_for_responses' crates/service/src/gateway/upstream/support/retry.rs
rg -n 'fn is_local_image_gen_tool' crates/service/src/gateway/request/official_responses_http.rs
```

Expected: 五组符号都存在。若缺失，停止执行并检查 merge 结果；不要直接复制旧文件。

### Task 3: 验证 experimental 网关判定

**Files:**
- Verify: `crates/service/src/codex_profile.rs`
- Test: `crates/service/src/codex_profile_tests.rs`

- [ ] **Step 1: 确认预期实现没有被新版覆盖**

Expected implementation shape:

```rust
if auth.as_deref().is_some_and(auth_json_is_gateway)
    || config
        .as_deref()
        .is_some_and(|content| detect_gateway_config(content).ok().flatten().is_some())
{
    return CodexProfileMode::Gateway;
}
```

`detect_gateway_config` 必须同时支持：

- `model_provider = "cm"` 的管理配置。
- 当前 provider 的有效 `base_url` 指向 CodexManager 服务地址。
- `localhost`、`127.0.0.1`、`::1` 等本机地址的等价比较。
- 非 CodexManager 地址返回 `None`。

- [ ] **Step 2: 运行 experimental 网关测试**

Run:

```bash
cargo test -p codexmanager-service experimental_gateway_config_overrides_login_tokens_and_stale_direct_marker -- --test-threads=1
cargo test -p codexmanager-service experimental_non_gateway_base_url_keeps_login_mode_direct -- --test-threads=1
```

Expected: 每条命令各运行 1 项测试并通过。

- [ ] **Step 3: 运行全部 Codex profile 测试**

Run:

```bash
cargo test -p codexmanager-service codex_profile::tests -- --test-threads=1
```

Expected: 全部通过，不出现数据库表或旧 marker 错误。

不创建新提交；旧 PR 已包含实现和测试。

### Task 4: 验证请求头、Responses 400 重试和旧地址禁用

**Files:**
- Verify: `crates/service/src/gateway/request/incoming_headers.rs`
- Verify: `crates/service/src/gateway/upstream/headers/codex_headers.rs`
- Verify: `crates/service/src/gateway/upstream/attempt_flow/transport.rs`
- Verify: `crates/service/src/gateway/upstream/attempt_flow/postprocess.rs`
- Verify: `crates/service/src/gateway/upstream/support/retry.rs`
- Test: matching `*_tests.rs` files

- [ ] **Step 1: 验证连字符请求头**

Expected implementation shape:

```rust
headers.push(("session-id".to_string(), session_id.to_string()));
headers.push(("thread-id".to_string(), thread_id.to_string()));
```

Run:

```bash
cargo test -p codexmanager-service current_codex_hyphenated_headers_are_captured -- --test-threads=1
cargo test -p codexmanager-service build_codex_upstream_headers_keeps_final_affinity_shape -- --test-threads=1
cargo test -p codexmanager-service build_codex_compact_upstream_headers_use_session_fallback_only -- --test-threads=1
```

Expected: 三条命令都通过；测试同时断言 `session_id` 和 `thread_id` 不存在。

- [ ] **Step 2: 验证无会话头发送方式**

Expected header filter:

```rust
matches!(
    name.to_ascii_lowercase().as_str(),
    "session-id"
        | "thread-id"
        | "x-client-request-id"
        | "x-codex-window-id"
        | "x-codex-turn-state"
        | "session_id"
)
```

Run:

```bash
cargo test -p codexmanager-service explicit_stateless_mode_targets_only_session_scoped_headers -- --test-threads=1
cargo test -p codexmanager-service explicit_stateless_mode_removes_session_id_added_by_gemini_profile -- --test-threads=1
```

Expected: 两项通过。第二项证明 Gemini/CPA profile 最后加入的会话头也会被移除。

- [ ] **Step 3: 验证官方 Responses 400 同地址重试**

Run:

```bash
cargo test -p codexmanager-service chatgpt_responses_400_retries_same_path_without_session_headers -- --test-threads=1
cargo test -p codexmanager-service chatgpt_responses_failed_stateless_retry_keeps_original_400 -- --test-threads=1
cargo test -p codexmanager-service bad_request_stateless_retry_requires_actual_responses_target -- --test-threads=1
```

Expected:

- 第一项确认第一次请求有会话头，第二次请求没有，并且两次使用同一 `/v1/responses` 地址。
- 第二项确认第二次仍失败时返回原始 400。
- 第三项确认其他请求不会触发该重试。

- [ ] **Step 4: 验证旧 Responses 地址不会被访问**

Run:

```bash
cargo test -p codexmanager-service gateway::upstream::support::retry::tests -- --test-threads=1
```

Expected: 以下行为同时通过：

- API 客户端 Responses 请求跳过旧地址。
- Codex 客户端 Responses 请求也跳过旧地址。
- compact 映射到 chat completions 时仍可使用它自己的备用地址。
- Anthropic bridge 仍可使用它自己的备用地址。
- 尾部带 `/` 的 Responses 地址仍能正确识别。

- [ ] **Step 5: 验证新版 Gemini、CPA 和 compact 规则**

Run:

```bash
cargo test -p codexmanager-service gemini_codex_compat_disables_request_compression_like_cpa -- --test-threads=1
cargo test -p codexmanager-service gemini_codex_compat_header_profile_matches_cpa_executor_shape -- --test-threads=1
cargo test -p codexmanager-service gemini_native_does_not_forward_thread_anchor_as_prompt_cache_key -- --test-threads=1
cargo test -p codexmanager-service responses_compact_uses_codex_compat_rewrite -- --test-threads=1
cargo test -p codexmanager-service responses_compact_keeps_only_codex_compact_body_fields -- --test-threads=1
```

Expected: 五项全部通过。

不创建新提交。若任一项失败，停止执行并按系统化排错流程确认新版调用路径，不把旧文件整体覆盖到新版。

### Task 5: 验证图像工具冲突修复

**Files:**
- Verify: `crates/service/src/gateway/request/official_responses_http.rs`
- Test: `crates/service/src/gateway/request/tests/request_rewrite_tests.rs`
- Test: `crates/service/tests/gateway_logs/images.rs`

- [ ] **Step 1: 确认本地图像工具判断**

Expected implementation:

```rust
fn is_local_image_gen_tool(tool: &Value) -> bool {
    let Some(tool_type) = tool
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tool_type| !tool_type.is_empty())
    else {
        return false;
    };
    let Some(name) = tool
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if tool_type.eq_ignore_ascii_case("namespace") {
        return name == "image_gen";
    }
    tool_type.eq_ignore_ascii_case("function")
        && (name == "image_gen"
            || name.starts_with("image_gen.")
            || name.starts_with("image_gen__"))
}
```

- [ ] **Step 2: 运行函数和 namespace 回归测试**

Run:

```bash
cargo test -p codexmanager-service responses_default_path_skips_auto_image_generation_when_image_gen_function_exists -- --test-threads=1
cargo test -p codexmanager-service responses_default_path_skips_auto_image_generation_when_image_gen_namespace_exists -- --test-threads=1
```

Expected: 两项通过，客户端工具被保留，托管 `image_generation` 没有追加。

- [ ] **Step 3: 确认无本地图像工具时仍会自动加入**

Run:

```bash
cargo test -p codexmanager-service responses_default_path_auto_injects_image_generation_tool_for_codex_backend -- --test-threads=1
cargo test -p codexmanager-service native_codex_responses_auto_injects_image_generation_tool -- --test-threads=1
```

Expected: 两项通过，原有图像生成功能没有被禁用。

- [ ] **Step 4: 运行全部请求改写测试**

Run:

```bash
cargo test -p codexmanager-service gateway::request_rewrite::tests -- --test-threads=1
```

Expected: 全部通过。

不创建新提交；旧 PR 已包含实现和测试。

### Task 6: 运行完整自动检查

**Files:**
- Verify: repository-wide

- [ ] **Step 1: 检查 Rust 格式**

Run:

```bash
cargo fmt --all -- --check
```

Expected: 退出码 0，没有格式差异。

- [ ] **Step 2: 运行前端检查与静态构建**

Run:

```bash
pnpm -C apps run lint
pnpm -C apps run test:runtime
pnpm -C apps run build:desktop
```

Expected:

- ESLint 通过。
- 运行时测试全部通过。
- Next.js 静态构建成功并生成 `apps/out`。

- [ ] **Step 3: 单线程运行整个 Rust workspace**

Run:

```bash
cargo test --workspace -- --test-threads=1
```

Expected: 所有 workspace 测试通过。使用单线程是因为该仓库有多个测试会修改进程级数据库路径。

- [ ] **Step 4: 运行 Tauri crate 测试**

Run:

```bash
cargo test --manifest-path apps/src-tauri/Cargo.toml -- --test-threads=1
```

Expected: 所有桌面端 Rust 测试通过。

- [ ] **Step 5: 确认测试没有产生未提交文件**

Run:

```bash
git status -sb
```

Expected: 分支仅领先远端，没有工作区修改。

### Task 7: 构建并检查 arm64 macOS 应用

**Files:**
- Build: `apps/src-tauri/target/aarch64-apple-darwin/release/bundle/`
- Verify: built `CodexManager.app`
- Verify: built DMG

- [ ] **Step 1: 准备 Rust arm64 target**

Run:

```bash
rustup target list --installed | rg '^aarch64-apple-darwin$' || rustup target add aarch64-apple-darwin
```

Expected: `aarch64-apple-darwin` 已安装。

- [ ] **Step 2: 使用仓库脚本构建 arm64 DMG**

Run:

```bash
env APPLE_SIGNING_IDENTITY=- bash scripts/rebuild-macos.sh --bundles "dmg" --target aarch64-apple-darwin --clean-dist
```

Expected:

- 命令成功，Tauri 在制作 DMG 前使用 identity `-` 对主程序和整个应用包完成 ad hoc 签名。
- 生成 v0.4.1 arm64 DMG。
- dmg-only 构建结束后，Tauri 会删除作为制盘中间产物的外部 `bundle/macos/CodexManager.app`，这是预期行为。
- 从此步骤开始，到 app-only 补构建完成前，不得修改源码、Tauri 配置、锁文件或依赖。

- [ ] **Step 3: 不清理 target，补构建外部应用包**

Run:

```bash
(
  cd apps/src-tauri
  env APPLE_SIGNING_IDENTITY=- cargo tauri build --bundles app --target aarch64-apple-darwin
)
```

Expected: 命令保留上一步已经生成的 DMG，并生成由 Tauri 完整 ad hoc 签名的外部 `CodexManager.app`。两次构建之间不得修改源码、配置、锁文件或依赖。

- [ ] **Step 4: 同时找到 APP 和 DMG**

Run:

```bash
APP_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type d -name 'CodexManager.app' | head -1)"
DMG_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type f -name '*.dmg' | head -1)"
test -n "$APP_PATH" && test -d "$APP_PATH"
test -n "$DMG_PATH" && test -f "$DMG_PATH"
realpath "$APP_PATH"
realpath "$DMG_PATH"
```

Expected: 同时输出外部 `CodexManager.app` 和 v0.4.1 arm64 DMG 的绝对路径。

- [ ] **Step 5: 检查外部应用的架构和 Info.plist**

Run:

```bash
APP_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type d -name 'CodexManager.app' | head -1)"
file "$APP_PATH/Contents/MacOS/CodexManager"
lipo -info "$APP_PATH/Contents/MacOS/CodexManager"
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :LSRequiresCarbon' "$APP_PATH/Contents/Info.plist"
plutil -lint "$APP_PATH/Contents/Info.plist"
```

Expected:

- `file` 和 `lipo` 都只报告 `arm64`，不出现 `x86_64`。
- 两个版本字段均为 `0.4.1`。
- `LSRequiresCarbon` 为 `false`。
- `plutil` 报告 `OK`。

- [ ] **Step 6: 只读核实外部应用已由 Tauri 完整签名**

Run:

```bash
APP_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type d -name 'CodexManager.app' | head -1)"
codesign -dv --verbose=4 "$APP_PATH" 2>&1
codesign --verify --deep --strict --verbose=4 "$APP_PATH"
```

Expected:

- `Identifier=com.codexmanager.desktop`。
- `Signature=adhoc`。
- `Sealed Resources version=2`。
- strict deep 验证退出码为 `0`，输出 `valid on disk` 和 `satisfies its Designated Requirement`。
- 不运行构建后的 `codesign --force`；签名必须来自 Tauri 打包流程。

- [ ] **Step 7: 只读挂载 DMG 并检查内部应用**

Run:

```bash
APP_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type d -name 'CodexManager.app' | head -1)"
DMG_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type f -name '*.dmg' | head -1)"
MOUNT_DIR="$(mktemp -d /tmp/codexmanager-dmg.XXXXXX)"
MOUNTED=false

cleanup() {
  if [[ "$MOUNTED" == "true" ]]; then
    hdiutil detach "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  rmdir "$MOUNT_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT_DIR" "$DMG_PATH"
MOUNTED=true
DMG_APP="$(find "$MOUNT_DIR" -maxdepth 2 -type d -name 'CodexManager.app' | head -1)"
test -n "$DMG_APP" && test -d "$DMG_APP"

file "$DMG_APP/Contents/MacOS/CodexManager"
lipo -info "$DMG_APP/Contents/MacOS/CodexManager"
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$DMG_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$DMG_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :LSRequiresCarbon' "$DMG_APP/Contents/Info.plist"
plutil -lint "$DMG_APP/Contents/Info.plist"
codesign -dv --verbose=4 "$DMG_APP" 2>&1
codesign --verify --deep --strict --verbose=4 "$DMG_APP"

cmp "$APP_PATH/Contents/MacOS/CodexManager" "$DMG_APP/Contents/MacOS/CodexManager"
cmp "$APP_PATH/Contents/Info.plist" "$DMG_APP/Contents/Info.plist"

hdiutil detach "$MOUNT_DIR"
MOUNTED=false
rmdir "$MOUNT_DIR"
trap - EXIT
! hdiutil info | rg -F "$MOUNT_DIR"
```

Expected:

- DMG 内主程序只包含 `arm64`。
- 两个版本字段均为 `0.4.1`，`LSRequiresCarbon=false`，`plutil` 报告 `OK`。
- 内部应用同样显示 `Identifier=com.codexmanager.desktop`、`Signature=adhoc`、`Sealed Resources version=2`，strict deep 验证退出码为 `0`。
- 内外主程序和 `Info.plist` 逐字节一致。
- DMG 已卸载，`hdiutil info` 中没有本次挂载点残留。

- [ ] **Step 8: 验证 DMG 文件完整性并说明分发限制**

Run:

```bash
DMG_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type f -name '*.dmg' | head -1)"
file "$DMG_PATH"
stat -f '%z %Sm' -t '%Y-%m-%d %H:%M:%S %z' "$DMG_PATH"
shasum -a 256 "$DMG_PATH"
hdiutil verify "$DMG_PATH"
```

Expected: `hdiutil verify` 退出码为 `0`，说明磁盘映像结构和校验和有效。它不证明发布者身份或下载来源可信。

本任务使用的 ad hoc 签名只用于本机安装和启动检查，Gatekeeper 不会把它当作可公开分发的可信包。公开分发必须改用 Developer ID Application 签名，完成 Apple notarization，并对最终应用或 DMG 执行 staple。

### Task 8: 备份、安装并检查应用窗口

**Files:**
- Backup: `/Applications/CodexManager.app.backup-<timestamp>`
- Install: `/Applications/CodexManager.app`

- [ ] **Step 1: 退出当前应用**

Run:

```bash
osascript -e 'tell application "CodexManager" to quit' || true
```

Expected: `/Applications/CodexManager.app/Contents/MacOS/CodexManager` 进程退出。

- [ ] **Step 2: 移走当前应用作为备份**

Run:

```bash
BACKUP_APP="/Applications/CodexManager.app.backup-$(date +%Y%m%d-%H%M%S)"
mv /Applications/CodexManager.app "$BACKUP_APP"
test -d "$BACKUP_APP"
```

Expected: 原应用完整保存在带时间的目录中。

- [ ] **Step 3: 安装 arm64 应用**

Run:

```bash
APP_PATH="$(find apps/src-tauri/target/aarch64-apple-darwin/release/bundle -type d -name 'CodexManager.app' | head -1)"
ditto "$APP_PATH" /Applications/CodexManager.app
file /Applications/CodexManager.app/Contents/MacOS/CodexManager
codesign --verify --deep --strict --verbose=2 /Applications/CodexManager.app
```

Expected: 安装后的主程序为 arm64，签名验证通过。

- [ ] **Step 4: 启动并检查窗口**

Run:

```bash
open /Applications/CodexManager.app
lsof -nP -iTCP:48760 -sTCP:LISTEN
```

Expected:

- CodexManager 主窗口正常显示，不是白屏。
- 页面菜单、设置页和平台模式页可以打开。
- 本地服务继续监听 `127.0.0.1:48760`。

如果启动失败，立即执行：

```bash
osascript -e 'tell application "CodexManager" to quit' || true
BACKUP_APP="$(find /Applications -maxdepth 1 -type d -name 'CodexManager.app.backup-*' | sort | tail -1)"
test -n "$BACKUP_APP"
mv /Applications/CodexManager.app /Applications/CodexManager.app.failed
mv "$BACKUP_APP" /Applications/CodexManager.app
open /Applications/CodexManager.app
```

### Task 9: 真实 RPC 与 Responses 请求检查

**Files:**
- Read: `$HOME/.codex/config.toml`
- Read: `$HOME/Library/Application Support/com.codexmanager.desktop/codexmanager.rpc-token`
- Read: `$HOME/Library/Application Support/com.codexmanager.desktop/codexmanager.db`
- Read: `$HOME/Library/Application Support/com.codexmanager.desktop/gateway-trace.log`

- [ ] **Step 1: 调用 Codex profile 状态 RPC**

Run:

```bash
RPC_TOKEN="$(tr -d '\r\n' < "$HOME/Library/Application Support/com.codexmanager.desktop/codexmanager.rpc-token")"
PROFILE_JSON="$(curl -sS http://127.0.0.1:48760/rpc \
  -H 'Content-Type: application/json' \
  -H "X-CodexManager-Rpc-Token: $RPC_TOKEN" \
  --data '{"jsonrpc":"2.0","id":1,"method":"codexProfile/get","params":{}}')"
jq -e '.result.mode == "gateway" and .result.gatewayBaseUrl == "http://localhost:48760/v1"' <<<"$PROFILE_JSON"
```

Expected: `jq` 返回成功。不要输出 `RPC_TOKEN`。

- [ ] **Step 2: 从现有配置读取网关 token，但不打印**

Run:

```bash
GATEWAY_TOKEN="$(python3 -c 'import pathlib,tomllib; p=pathlib.Path.home()/".codex"/"config.toml"; c=tomllib.loads(p.read_text()); provider=c.get("model_provider","default"); print(c.get("experimental_bearer_token") or c.get("model_providers",{}).get(provider,{}).get("experimental_bearer_token", ""))')"
test -n "$GATEWAY_TOKEN"
```

Expected: token 非空，终端不显示 token 内容。

- [ ] **Step 3: 发起最小 gpt-5.5 Responses 请求**

Run:

```bash
RESPONSE_JSON="$(curl --fail-with-body -sS http://127.0.0.1:48760/v1/responses \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_TOKEN" \
  --data '{"model":"gpt-5.5","input":"只回复 OK","stream":false}')"
jq -e '(.id // .response.id) and ([.. | objects | .error?] | all(. == null))' <<<"$RESPONSE_JSON"
```

Expected: HTTP 成功，返回 Responses 标识，不出现 `404 Not Found`。

- [ ] **Step 4: 发起带 image_gen namespace 的请求**

Run:

```bash
IMAGE_TOOL_JSON="$(curl --fail-with-body -sS http://127.0.0.1:48760/v1/responses \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_TOKEN" \
  --data '{"model":"gpt-5.5","input":"只回复 IMAGE TOOL OK，不调用工具","stream":false,"tools":[{"type":"namespace","name":"image_gen","description":"本地图像工具","tools":[{"type":"function","name":"imagegen","description":"生成图像","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"],"additionalProperties":false}}]}]}')"
jq -e '(.id // .response.id) and ([.. | objects | .error?] | all(. == null))' <<<"$IMAGE_TOOL_JSON"
```

Expected: 请求成功，不出现“托管 `image_generation` 与本地图像工具冲突”的 400。

- [ ] **Step 5: 查看最新请求路径**

Run:

```bash
sqlite3 "$HOME/Library/Application Support/com.codexmanager.desktop/codexmanager.db" \
  "SELECT request_path, upstream_url, status_code, error FROM request_logs WHERE model='gpt-5.5' ORDER BY id DESC LIMIT 5;"
rg -n '/backend-api/codex/v1/responses|responses_codex_v1_alt_blocked|gateway_responses_400' \
  "$HOME/Library/Application Support/com.codexmanager.desktop/gateway-trace.log" | tail -40
```

Expected:

- 最新请求状态成功。
- 新请求的上游地址不包含 `/backend-api/codex/v1/responses`。
- 日志中不得出现完整 token。

### Task 10: 推送分支并更新中文 PR

**Files:**
- Remote branch: `serein431:agent/local-combined-fixes`
- Pull request: `qxcnm/Codex-Manager#346`

- [ ] **Step 1: 最后检查分支和提交**

Run:

```bash
git status -sb
git log --oneline --decorate --graph -15
git diff --check origin/main...HEAD
```

Expected: 工作区干净，历史中包含九个旧提交、本次文档提交和官方 main merge commit。

- [ ] **Step 2: 推送现有分支**

Run:

```bash
git push fork agent/local-combined-fixes
```

Expected: 正常快进推送，不使用 `--force`。

- [ ] **Step 3: 使用中文标题和正文更新 PR #346**

Run:

```bash
gh pr edit 346 --repo qxcnm/Codex-Manager \
  --title '修复 Codex 网关判定、Responses 重试与图像工具冲突' \
  --body '## 问题

CodexManager 在保留 auth.json 现有信息、通过 experimental_bearer_token 使用本地网关时，profile 状态会被误判为 direct_account。部分 Codex Responses 请求还会在官方地址返回 400 后访问旧的 /backend-api/codex/v1/responses，最终把有用的 400 覆盖成 404。

真实请求还发现，客户端已经声明 image_gen 本地图像工具时，自动加入托管 image_generation 会造成工具冲突。macOS 包也需要明确声明不依赖 Carbon，并提供 arm64 构建。

## 修改

- 支持从当前 provider 的 base_url 和 experimental_bearer_token 配置识别 CodexManager 网关。
- 上游会话请求头使用 session-id 和 thread-id。
- 官方 /v1/responses 返回 400 时，在同一地址无会话头重试一次。
- Responses 请求不再访问旧的 /backend-api/codex/v1/responses。
- 第二次请求仍失败时保留原始 400 状态和正文。
- 保留新版 compact、Gemini、CPA、模型目录 v2、GPT-5.6 和 Codex Ultra 行为。
- 识别 image_gen namespace、image_gen、image_gen.* 和 image_gen__* 函数，避免重复加入托管图像工具。
- macOS Info.plist 写入 LSRequiresCarbon=false。

## 验证

- 前端 lint、运行时测试和静态构建通过。
- Rust workspace 单线程测试通过。
- Tauri crate 测试通过。
- Codex profile 状态 RPC 返回 gateway，并识别 http://localhost:48760/v1。
- gpt-5.5 最小 Responses 请求成功，不再出现旧地址 404。
- 带 image_gen namespace 的真实请求成功。
- 本机构建并安装的 CodexManager 主程序为 arm64，窗口正常显示。

## 说明

Rust 测试中有多个用例会修改进程级数据库路径，因此完整测试使用 --test-threads=1，避免并发环境变量互相影响。PR 中没有提交密钥、token 或本机登录文件。'
```

Expected: PR 标题和正文全部为中文。

- [ ] **Step 4: 检查 PR 当前状态**

Run:

```bash
gh pr view 346 --repo qxcnm/Codex-Manager \
  --json url,title,body,state,isDraft,headRefName,headRefOid,mergeable,statusCheckRollup
```

Expected:

- `state` 为 `OPEN`。
- `isDraft` 为 `false`。
- `headRefName` 为 `agent/local-combined-fixes`。
- 标题为中文新标题。
- `headRefOid` 等于本地 `HEAD`。

### Task 11: 最终验收

**Files:**
- Verify: local installation
- Verify: local branch
- Verify: PR #346

- [ ] **Step 1: 汇总可核实的最终状态**

Run:

```bash
git status -sb
file /Applications/CodexManager.app/Contents/MacOS/CodexManager
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/CodexManager.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :LSRequiresCarbon' /Applications/CodexManager.app/Contents/Info.plist
gh pr view 346 --repo qxcnm/Codex-Manager --json url,title,state,headRefOid
```

Expected:

- 本地工作区干净并与 fork 分支一致。
- 安装程序是 arm64。
- 版本为 0.4.1。
- `LSRequiresCarbon=false`。
- PR #346 已更新并指向本地最终提交。

- [ ] **Step 2: 保留备份直到用户确认**

不要自动删除 `/Applications/CodexManager.app.backup-<timestamp>`。只有用户确认新应用持续正常后，才询问是否清理旧应用备份。
