# Repository Engineering Standards

This file applies to the whole CodexManager repository. For work under `apps/`,
also read `apps/AGENTS.md`; that file contains the more specific frontend and
Tauri rules.

## 1. Project Shape
- `apps/`: Next.js frontend plus the Tauri desktop shell.
- `apps/src/`: App Router UI, components, hooks, API clients, runtime helpers,
  i18n, and Zustand state.
- `apps/src-tauri/`: Tauri v2 application shell, desktop lifecycle, tray/window
  behavior, native commands, and desktop RPC client code.
- `crates/core/`: SQLite migrations, storage primitives, auth helpers, and core
  usage/account data structures.
- `crates/service/`: local HTTP/RPC service, gateway routing, protocol adapters,
  account/API key/usage domains, plugins, app settings, and runtime sync.
- `crates/web/`: service-mode Web UI shell, embedded static UI serving, and
  `/api/runtime` / `/api/rpc` proxy behavior.
- `crates/start/`: service-mode launcher that starts service + web together.
- `scripts/`, `docker/`, `.github/`: build, release, probe, container, and CI
  automation.

## 2. Ownership Boundaries
- Keep UI behavior in `apps/src/`, desktop shell behavior in `apps/src-tauri/`,
  and service/gateway behavior in `crates/service/`.
- Put schema and persistence foundation changes in `crates/core/`, especially
  SQLite migrations and reusable storage helpers.
- Avoid expanding central entrypoints with unrelated orchestration. Large files
  should be treated as legacy surfaces; new substantial logic should move into
  focused modules, hooks, or domain helpers.
- Do not mix release/script changes with product behavior unless the task
  explicitly requires it.

## 3. API, RPC, and Command Sync
- Frontend code must call backend capabilities through typed wrappers in
  `apps/src/lib/api/`.
- Desktop IPC should use the centralized `invoke` / `invokeFirst` helpers from
  `@/lib/api/transport`; do not use raw `fetch()` for desktop commands.
- Service commands that require a service address should pass parameters through
  `withAddr()`. App-shell commands such as `app_*`, `open_*`, and window/update
  helpers may omit it when no service address is needed.
- Web/service-mode fallback is allowed only through the existing transport
  stack: `transport.ts`, `transport-web-commands.ts`, `rpc-http.ts`, and
  `fetchWithRetry`.
- When adding or renaming a backend command, keep the chain synchronized:
  Rust implementation, Tauri command registry when applicable, service RPC
  dispatch/Web command mapping when applicable, and the frontend API wrapper.
- Preserve the existing underscore command names and camelCase RPC method
  mapping conventions.

## 4. Settings and Persistence
- New persisted settings need explicit defaults, storage behavior, runtime sync
  behavior, and UI/API exposure.
- Check whether a setting affects desktop mode, service mode, web mode, or all
  three before choosing where to implement it.
- New `CODEXMANAGER_*` environment variables require documentation updates and
  should not bypass existing app settings unless startup-time behavior requires
  an environment-level setting.
- SQLite schema changes belong in `crates/core/migrations/` and should include
  storage-level tests when behavior is non-trivial.

## 5. Frontend and Desktop Rules
- Follow `apps/AGENTS.md` for Next.js, Tailwind, shadcn/Base UI, React Query,
  Zustand, glass theme, static export, and Tauri-specific rules.
- The frontend is statically exported for the desktop shell. Keep routing and
  asset paths compatible with `output: "export"` and `trailingSlash: true`.
- The Web UI must continue to work through `codexmanager-web`; a plain static
  page or ordinary Next dev server is not the complete service-mode runtime.

## 6. Rust Service Rules
- Keep gateway/protocol changes localized under `crates/service/src/gateway/`
  and `crates/service/src/http/` unless shared service state is genuinely needed.
- Protocol adapter changes must consider `/v1/responses`, `/v1/chat/completions`,
  streaming SSE, non-streaming JSON, tools, and `tool_calls`.
- Prefer typed request/response structs and existing storage helpers over ad hoc
  JSON or string manipulation.
- Web access, roles, billing/account mode, and API key ownership are security
  boundaries; do not weaken checks for UI convenience.

## 7. Validation
- Frontend-only changes: run at least `pnpm -C apps run build` and, when runtime
  behavior is touched, `pnpm -C apps run test:runtime`.
- Desktop/static-export changes: run `pnpm -C apps run build:desktop`.
- Rust/service changes: run `cargo test --workspace`, or the narrowest relevant
  package test only when the change is clearly isolated.
- Web shell/transport changes: add `cargo test -p codexmanager-web` and the
  relevant runtime probe scripts when available.
- Gateway/protocol changes require targeted regression coverage for streaming,
  non-streaming, tools, and both supported OpenAI-style endpoints.
- If a validation step cannot run in the current environment, record the exact
  command and the reason it was not executed.

## 8. Documentation
- Keep `README.md`, localized docs under `docs/`, and app-level docs aligned
  with user-visible behavior, deployment modes, environment variables, and
  release/build commands.
- Root governance docs describe repository-level boundaries. App-specific
  frontend rules belong in `apps/AGENTS.md`.

# 编码规范

## 语言
默认使用中文回答；除非用户明确要求使用其他语言。
所以代码的注释,输出,人类可读输出全部使用中文
## 代码质量
- 零容忍瑕疵代码，发现即改，不留任何潜在 bug
- 追求最优时间/空间复杂度，用最少代码解决最多问题
- 功能有问题直接删掉重写，禁止打补丁堆垃圾代码
- 不要任何兜底逻辑，底层直接解决根本问题
- 每次修 bug 后预防性排查全项目，相同或类似问题一并修复
- 禁止冗余代码：无用的 import、变量、函数、注释一律删除
- 禁止死代码：被注释掉的代码块、永远不会执行的分支、未被调用的函数必须删除
- 禁止复制粘贴式编程，重复逻辑必须抽象复用
- 任何问题代码,直接重构式修改,该删除重写的直接删除重写,不允许重复分支/临时兜底的补丁方式修改
- 所有代码函用最高效，简洁易懂的方式编写，不要把简单问题复杂化，一个功能各种补丁和分支兜底
- 命名必须精准自解释，禁止 tmp、data、handle 等模糊命名
- 单个函数职责应该单一,单个函数不宜超过200行,单个源码文件超过1500行即可考虑拆分，不宜超过2000行
- 嵌套不超过 3 层，超过则提取函数或提前 return
- 功能必须模块化，按职责拆分到独立文件/模块，禁止所有逻辑堆在一个文件里
- 优先使用成熟的开源库和标准库，不自己造轮子；只有开源方案确实不满足需求时才自行实现
- 你无法容忍已经发现的任何问题,任何导致代空间复杂度过高,时间复杂度过高的代码,发现立马重写或者修复
- 所有变量,函数,文件名均使用驼峰命名法,不使用_,等这些蛇形命名
- 所有变量,函数,结构体,方法源文件的命名均不能带上品牌名,按照功能和行业惯例来命名
- 代码的注释要规范,请参考网上成熟大厂的代码注释和编写风格
## 注释硬规则
- 所有新增或修改的函数必须写中文注释，说明函数职责、运行上下文、关键参数含义和失败返回语义
- 所有修复必须在对应代码附近写清楚修复背景、问题根因和修复理由，禁止只改代码不解释修复意图
- 注释必须描述当前设计意图，不允许写历史流水账、废话注释、被注释掉的旧代码块或没有跟进计划的 TODO
- 每次代码改动前后必须主动检查本次新增或修改的函数、结构体、宏和关键分支是否已补齐注释，不得以“代码自解释”为由省略函数注释或修复说明
- 若修复点没有合适的函数头位置，必须在最小相关代码块附近写清楚修复理由、影响范围和稳定性边界
## 代码整洁
- 不要把test测试代码混到业务代码文内，不要把test测试代码文件不要混到业务代码目录内
- 代码风格统一：缩进、空行、括号位置全项目一致
- 不写废话注释（"获取数据"、"返回结果"），只注释非显而易见的设计意图
- 魔法数字/字符串必须提取为常量
- 错误处理精确到位，不吞异常、不空 catch
- 参数不超过 4 个，超过则用结构体/对象封装
- 每个函数要注释清楚作用,每个系统功能或者模块必须要注释好背景,作用,为什么这么写,已经其中的技术科普
## 项目整洁
- 目录结构清晰分层，每个目录有明确职责
- 测试产生的临时文件（测试代码、文档、图片、日志等）确认无用后立即删除
- 不允许空文件、空目录、废弃文件残留在项目中
- 无 git 的项目自动初始化 git
- git 只跟踪有效源码，构建产物、临时文件、IDE 配置等必须加入 .gitignore
- 修改完毕后自动提交一次git
- 每次改动后检查是否引入了多余文件，有则清理

## 工程纪律
- 改完代码主动验证，不写未经验证的代码
- 修改公共模块时评估所有调用方的影响
- 不引入不必要的依赖，每个依赖必须有充分理由
-每次任务完成后会回复:本次代码任务遵守约束完成了:xxx
