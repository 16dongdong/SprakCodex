# Gateway request gate 300s blocking fix report

Date: 2026-05-24

## Summary

This change adds an opt-in mitigation and diagnostics for the request gate
head-of-line blocking path observed when many streaming `/v1/responses`
requests use the same API key and model. An unset
`CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS` keeps the legacy behavior: a
contending request may wait until the full stream request deadline, which
defaults to 300000 ms. Under high concurrency this can make queued requests
appear as 100s to 300s first-token latency and may end as 504 without ever
reaching an upstream account.

Operators that observe this behavior can set
`CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS=5000` to bound the gate wait. If the
gate is still busy after that bounded wait, the request records
`REQUEST_GATE_SKIP reason=gate_wait_timeout` and continues execution instead of
consuming the full request deadline inside the gate.

## Root Cause

- The request gate scope is keyed by API key, request path, and model.
- `/v1/responses` streaming requests have a default stream deadline of 300000
  ms.
- With `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS` unset or set to `0`, the
  gate wait is unbounded and capped only by the request deadline.
- In 12-concurrency reproduction runs before the fix, requests showed
  stair-step first-token latencies around 1s, 80s, 159s, and 240s, followed by
  multiple 300s 504s. Logs showed `REQUEST_GATE_SKIP reason=total_timeout
  wait_ms≈300000`.

Network quality is a separate risk. A poor egress path can still cause upstream
first-byte latency, 502s, or stream decode errors. This fix specifically removes
the code-level request-gate wait as the source of 300s first-token/504 behavior.

## Code Changes

- `crates/service/src/gateway/core/runtime_config.rs`
  - Keeps `DEFAULT_REQUEST_GATE_WAIT_TIMEOUT_MS=0` so the upstream-default
    behavior remains unchanged.
  - Keeps non-zero `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS` as the opt-in
    bounded-wait setting.
  - Keeps `0` as an explicit override value meaning unbounded, preserving the
    existing environment-variable contract.
- `crates/service/src/gateway/upstream/proxy_pipeline/request_gate.rs`
  - Keeps the request-gate wait capped by `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS`.
  - Emits `REQUEST_GATE_SKIP` with `wait_ms`.
  - Added a unit test proving a busy gate times out before a much longer request
    deadline.
- `crates/service/src/gateway/observability/trace_log.rs`
  - Logs `REQUEST_GATE_WAIT`, `REQUEST_GATE_ACQUIRED wait_ms`, and
    `REQUEST_GATE_SKIP wait_ms`.
  - Adds `first_response_ms` to `BRIDGE_RESULT`.
  - Adds `CODEXMANAGER_GATEWAY_TRACE_STDOUT_SLOW_MS` so slow successful traces
    can be flushed into Docker logs during diagnostics.
- Docker files and compose templates
  - Document `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS=5000` as an optional
    mitigation instead of enabling it by default.
  - Enable gateway trace/error stdout logging for container diagnostics.

## Local Validation

Commands run from repository root:

| Command | Result |
| --- | --- |
| `cargo fmt -- --check` | attempted on the rebased branch; latest `upstream/main` has unrelated compact-model formatting diffs outside this PR scope |
| `cargo test -p codexmanager-service request_gate --lib` | exit 0, 7 passed, 0 failed, 0 ignored |
| `cargo test -p codexmanager-service trace_log --lib` | exit 0, 8 passed, 0 failed, 0 ignored |
| `cargo check -p codexmanager-service --all-targets` | exit 0 |

## Test Image

- Image tag:
  `registry.cn-hangzhou.aliyuncs.com/kilimiao/codex-manager:0.3.4-test-request-gate-fix-20260523193202`
- Local image ID:
  `sha256:b66ac9d12b304f9bcf89294ee4ca88128cd02f4e66a691cd5063408f4b76c657`
- Registry digest:
  `sha256:caaa3ba1819e714a02707e5868e0677b0a5f60382ebc361be9aed4215b9c9b9e`

The image is explicitly a test image and does not overwrite the official
`v0.3.4` tag.

## Local Docker Test Setup

Container:

- Name: `codexmanager-gate-test`
- Image:
  `registry.cn-hangzhou.aliyuncs.com/kilimiao/codex-manager:0.3.4-test-request-gate-fix-20260523193202`
- Image ID:
  `sha256:b66ac9d12b304f9bcf89294ee4ca88128cd02f4e66a691cd5063408f4b76c657`
- Health: `healthy`
- Data directory: `/private/tmp/codexmanager-gate-test-data`
- Accounts imported from local backup: 5 total, 5 created, 0 failed
- Active API keys in test DB: 1
- Proxy:
  `CODEXMANAGER_UPSTREAM_PROXY_URL=http://host.docker.internal:7890`
- Gate config used for the pressure test:
  `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS=5000`

The test container used the host Stash proxy. This differs from the previous
company-server network path, so these Docker pressure results prove the request
gate behavior under a healthy upstream path, not company-server egress quality.

## Pressure Test Results

Each round used:

- Same API key
- Same path: `/v1/responses`
- Same model: `gpt-5.5`
- Same request shape: streaming, `reasoning.effort=high`
- Concurrency: 12
- Prompt size: 126041 characters

| Round | Status counts | first_body_ms p50 / p95 / max | total_ms p50 / p95 / max | 300s 502/504 | `REQUEST_GATE_SKIP total_timeout` | gate wait >= 100s | max gate wait |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 200 x 12 | 7159 / 7163 / 7308 | 9119 / 11588 / 12374 | 0 | 0 | 0 | 5002 ms |
| 2 | 200 x 12 | 6838 / 7111 / 7706 | 9634 / 10385 / 10610 | 0 | 0 | 0 | 5007 ms |
| 3 | 200 x 12 | 7008 / 7346 / 8235 | 9758 / 10629 / 11834 | 0 | 0 | 0 | 5004 ms |

Validation criteria:

- `REQUEST_GATE_SKIP reason=total_timeout`: 0 across all 3 rounds.
- Gate wait `>= 100000 ms`: 0 across all 3 rounds.
- 300s-range 504: 0 across all 3 rounds.
- 36/36 requests returned 200.

Docker logs show expected bounded-gate behavior:

- First request in a batch commonly acquires the gate immediately.
- Some contending requests acquire the gate after a short wait.
- Remaining contenders skip the gate at about 5000 ms with
  `reason=gate_wait_timeout` and continue upstream execution.
- No contender waits until the 300000 ms request deadline.

## Rollback

This local verification used a separate test container and data directory. To
remove it:

```bash
docker stop codexmanager-gate-test
docker rm codexmanager-gate-test
```

For any environment that deploys this test image through Compose, rollback means
changing the image back to the previously deployed tag or digest, then running:

```bash
docker compose pull
docker compose up -d
```

Do not keep the diagnostic stdout logging settings enabled permanently unless
they are needed for a short investigation window.

## Remaining Risk

- Company-server egress is not validated by the local Stash-proxied Docker run.
  If that network path is unstable, upstream first-byte delays or 502s can still
  occur.
- The new logs make the distinction explicit:
  - Gate problem: `REQUEST_GATE_SKIP reason=total_timeout` or
    `wait_ms>=100000`.
  - Network/upstream problem: gate wait is bounded, then `ATTEMPT_RESULT` or
    `BRIDGE_RESULT first_response_ms` shows upstream delay/error.
- Because the default remains legacy-compatible, operators must explicitly set a
  non-zero `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS` value to enable bounded
  gate waits in high-concurrency deployments.
- `CODEXMANAGER_REQUEST_GATE_WAIT_TIMEOUT_MS=0` still intentionally means
  unbounded wait.
