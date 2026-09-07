# Codex Responses 400 Retry Design

## Problem

For ChatGPT Codex Responses requests, the canonical endpoint can return `400 Bad Request` when session-scoped request headers are rejected. CodexManager currently tries the legacy `/backend-api/codex/v1/responses` path next. That path returns `404 Not Found`, and the 404 response replaces the useful canonical 400 response.

The existing `strip_session_affinity` flag is not a fully stateless mode. It can still rebuild `session-id` from the request body and can still emit `thread-id`, `x-client-request-id`, and `x-codex-window-id`.

## Selected behavior

When a ChatGPT-backed `/v1/responses` request returns 400:

1. Retry the same canonical URL once.
2. Remove `session-id`, `thread-id`, `x-client-request-id`, `x-codex-window-id`, and `x-codex-turn-state` from that retry.
3. Replace the original response only when the retry succeeds.
4. Do not try the legacy `/backend-api/codex/v1/responses` path for Responses requests.
5. If the stateless retry also fails, return the original canonical response so its status and body remain visible.

This behavior is separate from `strip_session_affinity`, because that flag is also used when moving a request between accounts and intentionally retains a stable fallback session.

## Code boundaries

- `transport.rs` owns the explicit no-session-header send mode.
- `postprocess.rs` decides when a canonical 400 gets one no-session-header retry and preserves the original response on failure.
- `support/retry.rs` blocks the obsolete Codex v1 alternate path for all Responses requests.
- Existing request construction and account selection behavior remain unchanged.

## Tests

- A transport-level test checks that the five session-scoped headers are removed together.
- A postprocess integration test sends a 400 followed by a 200 and checks that only the second request omits those headers.
- A postprocess integration test sends a canonical 400 followed by another failure and checks that the final response stays 400 and the legacy path is not requested.
- Existing service tests run after the focused tests.

## Live replay follow-up

After the 404 masking fix was installed, replaying the original `gpt-5.5` turn exposed the canonical 400 body: CodexManager had auto-injected the hosted `image_generation` tool while the client already declared the local image-generation capability. The current Codex client sends it as a namespace tool named `image_gen`, which the upstream expands to the function `image_gen.imagegen`. The upstream rejects that combination.

When image-generation auto-injection is enabled, CodexManager must keep the client's `image_gen` namespace or local `image_gen`, `image_gen.*`, or `image_gen__*` function tool and skip adding the hosted `image_generation` tool. Requests without a local image-generation tool keep the existing auto-injection behavior.
