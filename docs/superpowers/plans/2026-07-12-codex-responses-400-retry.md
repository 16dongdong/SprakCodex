# Codex Responses 400 Retry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retry a canonical ChatGPT Responses 400 once without session-scoped headers and prevent a legacy-path 404 from replacing the canonical error.

**Architecture:** Add an explicit transport mode that removes the five session-scoped headers after the normal Codex header profile is built. Invoke it from postprocessing only for canonical ChatGPT Responses 400 responses, and keep the original response unless the retry succeeds. Disable the legacy Codex v1 alternate path for Responses requests.

**Tech Stack:** Rust, reqwest blocking client, tiny_http test server, Cargo test.

---

### Task 1: Prove the missing no-session-header mode

**Files:**
- Modify: `crates/service/src/gateway/upstream/attempt_flow/transport_tests.rs`
- Modify: `crates/service/src/gateway/upstream/attempt_flow/postprocess_tests.rs`

- [ ] **Step 1: Add a transport test for the five headers**

Add a test that builds a normal Codex request with `session-id`, `thread-id`, `x-client-request-id`, `x-codex-window-id`, and `x-codex-turn-state`, sends it through the wished-for no-session-header transport function, captures the outgoing request, and asserts that all five names are absent.

- [ ] **Step 2: Add an integration test for 400 then 200**

Use a two-request `tiny_http` server. Return 400 to the initial request and 200 to the retry. Assert that the first request has the five headers, the second request does not, the same canonical URL is used twice, and the final decision contains status 200.

- [ ] **Step 3: Run the focused tests and verify RED**

Run:

```bash
cargo test -p codexmanager-service gateway::upstream::attempt_flow::postprocess::tests::chatgpt_responses_400_retries_same_path_without_session_headers -- --exact
```

Expected: compilation or assertion failure because the explicit no-session-header retry does not exist.

### Task 2: Add the explicit transport mode

**Files:**
- Modify: `crates/service/src/gateway/upstream/attempt_flow/transport.rs`
- Test: `crates/service/src/gateway/upstream/attempt_flow/transport_tests.rs`

- [ ] **Step 1: Add a header-name predicate**

Add a private predicate matching these case-insensitive names:

```rust
matches!(
    name.to_ascii_lowercase().as_str(),
    "session-id"
        | "thread-id"
        | "x-client-request-id"
        | "x-codex-window-id"
        | "x-codex-turn-state"
)
```

- [ ] **Step 2: Add a dedicated sender**

Add `send_upstream_request_without_session_headers(...)`. It calls the shared transport implementation with a `drop_session_headers` flag set to true. Existing send functions pass false.

- [ ] **Step 3: Remove the headers after profile construction**

After `build_codex_upstream_headers` or `build_codex_compact_upstream_headers`, retain only headers for which the new predicate is false when `drop_session_headers` is true.

- [ ] **Step 4: Run the transport test and verify GREEN**

Run:

```bash
cargo test -p codexmanager-service gateway::upstream::attempt_flow::transport::tests::transport_explicit_stateless_mode_drops_all_session_headers -- --exact
```

Expected: PASS.

### Task 3: Retry the canonical path and keep the original failure

**Files:**
- Modify: `crates/service/src/gateway/upstream/attempt_flow/postprocess.rs`
- Modify: `crates/service/src/gateway/upstream/attempt_flow/postprocess_tests.rs`

- [ ] **Step 1: Add the canonical retry helper**

Add a helper that triggers only when the upstream base is ChatGPT Codex, the request path starts with `/v1/responses`, and the current status is 400. It calls `send_upstream_request_without_session_headers` with the canonical URL and returns a response only when that response is successful.

- [ ] **Step 2: Invoke it before alternate-path handling**

Call the helper before `retry_with_alternate_path`. Update `upstream`, `status`, content type, and `cf-ray` only when the retry succeeds. Otherwise leave the original response untouched.

- [ ] **Step 3: Run the 400 then 200 test and verify GREEN**

Run:

```bash
cargo test -p codexmanager-service gateway::upstream::attempt_flow::postprocess::tests::chatgpt_responses_400_retries_same_path_without_session_headers -- --exact
```

Expected: PASS.

### Task 4: Block the obsolete Responses alternate path

**Files:**
- Modify: `crates/service/src/gateway/upstream/support/retry.rs`
- Modify: `crates/service/src/gateway/upstream/support/retry_tests.rs`
- Modify: `crates/service/src/gateway/upstream/attempt_flow/postprocess_tests.rs`

- [ ] **Step 1: Change the alternate-path guard test to RED**

Change the native Codex test so `/v1/responses` with an alternate URL containing `/backend-api/codex/v1/` must be skipped. Add an integration test where both canonical attempts fail and assert that the final status remains the first 400 with only two canonical requests observed.

- [ ] **Step 2: Simplify the guard**

Make the guard depend on the Responses request path and obsolete alternate URL, regardless of client identity headers.

- [ ] **Step 3: Run the focused tests and verify GREEN**

Run:

```bash
cargo test -p codexmanager-service gateway::upstream::support::retry::tests -- --nocapture
cargo test -p codexmanager-service gateway::upstream::attempt_flow::postprocess::tests -- --nocapture
```

Expected: PASS.

### Task 5: Validate, package, and publish

**Files:**
- Modify only if required by build failures found in the changed code.

- [ ] **Step 1: Run formatting and focused service tests**

```bash
cargo fmt --all -- --check
cargo test -p codexmanager-service gateway::upstream::attempt_flow::transport::tests -- --nocapture
cargo test -p codexmanager-service gateway::upstream::attempt_flow::postprocess::tests -- --nocapture
cargo test -p codexmanager-service gateway::upstream::support::retry::tests -- --nocapture
```

- [ ] **Step 2: Run the service package tests**

```bash
cargo test -p codexmanager-service
```

- [ ] **Step 3: Build the arm64 application**

Use the repository's existing desktop build command and confirm the output executable is Mach-O arm64 before replacing `/Applications/CodexManager.app`.

- [ ] **Step 4: Replay the original failed task once**

Replay task `019f47d4-2220-73d2-a72e-f54192d20e7b`. Confirm the request no longer ends with the legacy-path 404. Record the canonical retry statuses and verify that the stateless retry omits the five session-scoped headers.

- [ ] **Step 5: Commit and update the pull request**

Commit only the focused gateway, tests, and documentation changes. Push the existing branch and update the open CodexManager pull request, or create a new pull request if this fix is intentionally separate.

### Task 6: Prevent hosted image-tool conflicts exposed by the replay

**Files:**
- Modify: `crates/service/src/gateway/request/official_responses_http.rs`
- Modify: `crates/service/src/gateway/request/tests/request_rewrite_tests.rs`

- [ ] **Step 1: Reproduce the conflict in a request rewrite test**

Enable `CODEXMANAGER_CODEX_IMAGE_GENERATION_AUTO_INJECT_TOOL`, then cover both known client forms: a function named `image_gen.imagegen` and a namespace named `image_gen`. Assert that the rewritten request keeps the client tool without adding the hosted tool.

- [ ] **Step 2: Skip hosted-tool injection for local image-gen functions**

Before appending `{ "type": "image_generation" }`, scan existing tools. Skip the append for the `image_gen` namespace and for function names equal to `image_gen`, starting with `image_gen.`, or starting with `image_gen__`.

- [ ] **Step 3: Verify existing image-tool behavior**

Run:

```bash
cargo test -p codexmanager-service gateway::request_rewrite::tests -- --nocapture
```

Expected: the conflict regression test and the existing auto-injection tests all pass.
