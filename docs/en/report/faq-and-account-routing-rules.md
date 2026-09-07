# FAQ and Account Routing Rules

## Common issues
- Authorization callback fails: first check whether `CODEXMANAGER_LOGIN_ADDR` is already in use, or use manual callback parsing in the UI.
- Model list or request is blocked by a challenge page: first check your proxy exit, header differences, and account status. Do not fall back to the old compatibility path or upstream-cookie troubleshooting.
- Still blocked by Cloudflare / WAF: stop following the old compatibility path. Troubleshooting should now follow the `Codex-First` direction only.
- `Some data failed to refresh, available data is shown` appears frequently: automatic refresh now only logs failures; manual refresh shows failed items and sample errors. Check the `Background Tasks` settings and the service logs.
- Running service / web standalone: if the current directory is not writable, such as an install directory, set `CODEXMANAGER_DB_PATH` to a writable path.
- `502/503` on macOS behind a proxy: make sure the system proxy does not intercept local loopback traffic (`localhost/127.0.0.1` should go through `DIRECT`), and make sure the address uses lowercase `localhost:<port>`.

## Migration notes

### `Codex-First`

The repository has now settled on a `Codex-First` direction:

- The old compatibility path is no longer the primary route.
- Each session binds to only one active account.
- Manual account switching changes the upstream thread, and automatic switching does the same.

### Current behavior vs target behavior

Current behavior:

- `balanced` still rotates strictly by `Key + Model`.
- Session-to-account binding is still moving from `per-request rotation` to `session binding`.

Target behavior:

- Once a session is bound, it should prefer the bound account instead of entering normal rotation.
- Automatic account switching should also switch the upstream thread generation.
- Legacy compatibility switches should completely leave the main path and stop being recommended settings.

## Account routing rules
- In `ordered` mode, the gateway builds candidates by ascending account `sort` and tries them one by one, for example `0 -> 1 -> 2 -> 3`.
- This means `try in order`, not `always hit account 0`. If earlier accounts are unavailable or fail, the gateway automatically moves to the next one.

### Common reasons why earlier accounts are skipped
- The account status is not `active`
- The account has no token
- Usage evaluation marks the account unavailable, for example the main quota window is exhausted or usage fields are missing
- The account is in cooldown, or a soft concurrency limit caused it to be skipped

### `balanced` mode
- By default, `balanced` rotates strictly across available accounts on the `Key + Model` dimension, so it does not guarantee starting from the smallest `sort`.
- Only when you explicitly increase `CODEXMANAGER_ROUTE_HEALTH_P2C_BALANCED_WINDOW` will it add health-based reshuffling on top of balanced rotation.

## Trace logs
You can inspect `gateway-trace.log` in the same directory as the database:
- `CANDIDATE_POOL`: candidate order for the current request
- `CANDIDATE_START` / `CANDIDATE_SKIP`: actual attempts and skip reasons
- `REQUEST_FINAL`: the final matched account

## Related documents
- Environment and runtime configuration: [Environment and Runtime Configuration](environment-and-runtime-config.md)
- Minimal troubleshooting guide: [Minimal Troubleshooting Guide](minimal-troubleshooting-guide.md)
