# CodexManager v0.4.1 旧 PR 移植设计

## 目标

把 PR #346 的九个修复提交移到官方当前 `main`，保留 v0.4.1 以来的新功能与目录结构，并继续使用原 PR 和原分支。完成后构建 arm64 macOS 应用，替换本机的 x86_64 版本，并用真实请求确认 `experimental_bearer_token` 配置能被识别为网关模式，`/v1/responses` 不再被旧地址的 404 覆盖。

这次工作不改变 `auth.json` 中已有的登录信息，不添加新的切换方式，也不做与本问题无关的改造。

## 当前依据

- 旧 PR 分支为 `serein431:agent/local-combined-fixes`，当前提交为 `5a661cc`。
- 官方当前 `main` 为 `514f3db`，v0.4.1 标签为 `c3409c2`。
- `git cherry origin/main fork/agent/local-combined-fixes` 表明九个旧提交都没有进入官方分支。
- 对 `HEAD` 和 `origin/main` 的 `git merge-tree --write-tree` 预演返回成功，没有文本冲突。不过双方都修改了网关请求、请求头和 Codex profile 文件，仍需逐项检查行为，不能只看 Git 是否报冲突。
- 基线检查结果：
  - `pnpm -C apps run test:runtime`：102 项通过。
  - `pnpm -C apps run build:desktop`：通过，并生成 `apps/out`。
  - Rust 服务与网关测试在 `--test-threads=1` 下通过。
  - `cargo test -p codexmanager-web -- --test-threads=1`：21 项通过。
- Rust 全套测试并发执行时有两个共享数据库配置相关的偶发失败；两个失败单独运行均通过。后续验收使用单线程，避免把进程级环境变量干扰误判为代码错误。

## 合并方式

1. 在现有 `agent/local-combined-fixes` 分支上执行普通 merge，把 `origin/main` 合入旧 PR 分支。
2. 保留九个旧提交和一个新的 merge commit，不变基旧提交，不强制推送。
3. Git 没有文本冲突时，也要查看共同修改的文件，确认新版行为没有被旧实现覆盖。
4. 新版代码优先。若旧修复所处的函数或流程已经变化，只移植仍需要的行为与测试，不整文件替换。
5. 修改完成后推送同一远端分支，继续更新 PR #346，不另开 PR。

## 需要保留的九个修复

| 提交 | 需要保留的行为 |
| --- | --- |
| `4bd2e3d` | 发往 Codex 上游的会话请求头使用 `session-id` 和 `thread-id`，不再生成下划线形式。 |
| `9ff9240` | 当 `config.toml` 的 `experimental_bearer_token` 与 CodexManager 网关配置有效时，即使保留登录 token，也识别为网关模式；普通的非网关本地地址仍保持原判定。 |
| `a305696` | macOS 包中的 `LSRequiresCarbon` 明确为 `false`，避免系统把应用判为需要旧 Carbon 环境。 |
| `39c4c38` | 保留 Responses 400 重试的设计说明。 |
| `3959ba5` | 官方 `/v1/responses` 返回 400 时，在同一地址重试一次，并去掉会话相关请求头。 |
| `6259298` | 上述重试不破坏 compact 请求和 Gemini 相关行为。 |
| `e51bd86` | Responses 请求不再尝试旧的 `/backend-api/codex/v1/responses` 地址；第二次请求仍失败时返回最初的 400。 |
| `3eac0f0` | 客户端已经声明本地图像工具时，不再自动加入托管 `image_generation` 工具。 |
| `5a661cc` | 同时识别 `image_gen` namespace、`image_gen`、`image_gen.*` 和 `image_gen__*` 函数名。 |

## 新版代码必须保留的部分

- 保留 v0.4.1 的版本号、前端改动、更新器和按架构选择 macOS 安装包的代码。
- 保留模型目录 v2、GPT-5.6 元数据与 Codex Ultra 分类。
- 保留 `candidate_executor.rs` 中现有的 400 重试流程。旧 PR 的“同地址、无会话头重试”只能作为受限补充，不能取代新版对 turn state、加密内容和候选请求的处理。
- 保留 Gemini 原生请求不把 thread anchor 转成 `prompt_cache_key` 的规则。
- 保留新版对 Gemini CLI 等非 Codex 客户端身份请求头的兼容处理。
- 保留新版 compact 请求的专用路径、请求体处理和失败处理。
- `Info.plist` 要经过实际 arm64 包检查。只有确认 Tauri 构建确实把 `LSRequiresCarbon=false` 写进应用包后，才认为该修复有效。

## 实施方法

### 1. 合入官方 main

执行普通 merge 后，先查看合并提交和共同修改文件的差异。重点检查：

- `crates/service/src/codex_profile.rs`
- `crates/service/src/codex_profile_tests.rs`
- `crates/service/src/gateway/request/`
- `crates/service/src/gateway/upstream/attempt_flow/`
- `crates/service/src/gateway/upstream/headers/`
- `crates/service/src/gateway/upstream/proxy_pipeline/`
- `crates/service/src/gateway/upstream/support/retry.rs`
- `apps/src-tauri/Info.plist`
- `apps/src-tauri/tauri.conf.json`

### 2. 先跑旧修复的测试

合并后先运行九个提交带来的测试。若测试因为新版结构变化而不能编译或失败，先确认新版调用流程，再修改最少的代码让原行为恢复。每个行为都要有独立测试，不把多个问题放进一个补丁里试。

### 3. 检查请求流程

Responses 400 的处理顺序应为：

1. 正常请求官方 `/v1/responses`。
2. 仅在符合条件的 400 上，对同一地址发起一次无会话头重试。
3. 重试成功才替换原响应。
4. 重试仍失败时保留原始 400 状态和正文。
5. 全程不访问 `/backend-api/codex/v1/responses`。

无会话头重试必须去掉：

- `session-id`
- `thread-id`
- `x-client-request-id`
- `x-codex-window-id`
- `x-codex-turn-state`

该重试不能影响 compact、Gemini、Anthropic 适配和已有候选请求重试。

### 4. 检查图像工具

请求体已有以下任一种本地图像能力时，保留客户端工具，不自动加入托管 `image_generation`：

- namespace 为 `image_gen`
- 函数名为 `image_gen`
- 函数名以 `image_gen.` 开头
- 函数名以 `image_gen__` 开头

请求体没有本地图像工具时，保持现有自动加入行为。

### 5. 检查 profile 判定

使用临时目录构造真实的 `auth.json` 和 `config.toml`：

- 保留登录 token，并在顶层和当前 provider 中写入网关 `base_url` 与 `experimental_bearer_token`，期望返回 `gateway`。
- 写入普通的其他本地地址和 token，期望保持 `direct_account`。
- 使用旧 marker 与新配置组合，确认有效配置优先于过期 marker。

测试和日志不得输出完整 token。

## 验证方案

### 自动测试

按以下顺序执行：

```bash
pnpm -C apps run test:runtime
pnpm -C apps run build:desktop
cargo fmt --all -- --check
cargo test -p codexmanager-service codex_profile::tests -- --test-threads=1
cargo test -p codexmanager-service gateway::upstream::headers::codex_headers::tests -- --test-threads=1
cargo test -p codexmanager-service gateway::upstream::attempt_flow::postprocess::tests -- --test-threads=1
cargo test -p codexmanager-service gateway::upstream::attempt_flow::transport::tests -- --test-threads=1
cargo test -p codexmanager-service gateway::upstream::support::retry::tests -- --test-threads=1
cargo test -p codexmanager-service gateway::request::tests::request_rewrite_tests -- --test-threads=1
cargo test --workspace -- --test-threads=1
cargo test --manifest-path apps/src-tauri/Cargo.toml -- --test-threads=1
```

如果新版模块名变化，实施计划应改成实际测试路径，但测试内容不能减少。

### 真实运行检查

在不修改现有登录信息的前提下：

1. 启动新构建的 CodexManager 服务。
2. 调用 Codex profile 状态 RPC，确认返回 `gateway`，并显示 `http://localhost:48760/v1`。
3. 发起一个最小 `/v1/responses` 请求，确认不再出现旧地址 404，并得到有效 Responses 结果。
4. 发起带 `image_gen` namespace 的请求，确认没有托管图像工具冲突。
5. 检查请求日志中的实际上游路径、两次请求状态和请求头过滤结果；日志中不得保留密钥。

## arm64 构建与安装

1. 安装或确认 `aarch64-apple-darwin` Rust target。
2. 使用仓库脚本构建：

   ```bash
   env APPLE_SIGNING_IDENTITY=- bash scripts/rebuild-macos.sh --bundles "dmg" --target aarch64-apple-darwin --clean-dist
   ```

   Tauri 必须在制作 DMG 前使用 identity `-` 对主程序和整个应用包完成 ad hoc 签名。dmg-only 构建结束后会删除外部 `CodexManager.app`。

3. 在不修改源码、Tauri 配置、锁文件或依赖的情况下，不清理 target，补构建外部应用包并保留已经生成的 DMG：

   ```bash
   (
     cd apps/src-tauri
     env APPLE_SIGNING_IDENTITY=- cargo tauri build --bundles app --target aarch64-apple-darwin
   )
   ```

4. 同时找到外部 APP 和 DMG。使用 `file` 或 `lipo -info` 确认内外主程序都只有 arm64，不接受 x86_64 产物。
5. 使用 `plutil` 和 `PlistBuddy` 检查内外应用的最终 `Info.plist`，确认版本为 `0.4.1`、`LSRequiresCarbon=false`，并比较内外主程序与 `Info.plist` 完全一致。
6. 只读核实内外应用均由 Tauri 完整签名：`Identifier=com.codexmanager.desktop`、`Signature=adhoc`、`Sealed Resources version=2`，且 `codesign --verify --deep --strict` 退出码为 `0`。不得在构建后运行 `codesign --force` 手工补签。
7. 只读挂载 DMG 完成内部检查，最后卸载并确认没有挂载残留。运行 `hdiutil verify` 检查磁盘映像结构和校验和，但不得把该结果描述为发布者身份或下载来源可信。
8. ad hoc 签名只用于本机安装和启动检查，Gatekeeper 不接受它作为公开分发凭据。公开分发需要 Developer ID Application 签名、Apple notarization 和 staple。
9. 退出正在运行的 CodexManager，为 `/Applications/CodexManager.app` 建立带时间的备份，再从外部 APP 路径安装新应用。
10. 启动后确认版本、架构、窗口显示和本地 RPC；若启动失败，立即恢复备份。

## PR 更新

继续使用 PR #346，建议中文标题：

> 修复 Codex 网关判定、Responses 重试与图像工具冲突

PR 正文使用中文，包含：

- 问题表现与原因。
- 九个修复在新版中的保留方式。
- 新版功能没有被覆盖的说明。
- 自动测试结果、真实请求结果和 arm64 应用检查结果。
- 已知的并发测试环境干扰说明。

PR 正文、测试输出摘要和评论中不得出现密钥、完整 token 或本机登录文件内容。

## 完成条件

- 原 PR 分支包含官方当前 `main` 和九个修复。
- 所有指定测试通过。
- profile 状态 RPC 返回网关模式。
- 真实 `/v1/responses` 请求不再返回旧地址 404。
- `image_gen` 请求不再产生托管工具冲突。
- 安装后的 CodexManager 主程序为 arm64，能正常显示窗口。
- PR #346 的标题和正文已经改为中文。
