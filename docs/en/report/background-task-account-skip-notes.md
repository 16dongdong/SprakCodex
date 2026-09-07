# Background task account filter table

Below we only look at "where to filter what status" to facilitate local troubleshooting.

| Tasks | Filter Locations | Filter Conditions | Skipped Status/Results | Description |
| --- | --- | --- | --- | --- |
| Usage polling thread | [crates/service/src/usage/refresh/batch.rs](../../../crates/service/src/usage/refresh/batch.rs#L91) | `status = disabled`, and the latest event hit a ban or refresh-blocked reason | `disabled`, `account_deactivated`, `workspace_deactivated`, `refresh_token_region_blocked` | Put these accounts into `skipped_ids` first, and then filter them out from the token list |
| Gateway keep-alive thread | [crates/core/src/storage/accounts.rs](../../../crates/core/src/storage/accounts.rs#L117) | `status = active`, and meet the gateway availability conditions | Accounts other than `active` will not enter the candidate set directly | Instead of traversing all accounts, candidate accounts will be screened first and then kept alive |
| Token refresh polling | [crates/core/src/storage/tokens.rs](../../../crates/core/src/storage/tokens.rs#L28) | `refresh_token` cannot be empty; the latest status cannot be a deactivated class reason or `refresh_token_region_blocked` | None `refresh_token`, `account_deactivated`, `workspace_deactivated`, `refresh_token_region_blocked` | Only process tokens that can be refreshed and are not paused; after fixing the proxy route, manually restoring the account makes it eligible again |

## Supplement

| Phenomenon | Common causes |
| --- | --- |
| It seems "available one second, expired the next second" | The server will not return until the next refresh `refresh_token_expired` |
| There is no action for gateway keepalive | The candidate account is empty, or there is no `active` available account |
| No movement in usage polling | The accounts have been filtered out, or there are no accounts that can be processed in the current batch |

## Shortest conclusion

Both `用量轮询` and `令牌刷新` will filter accounts by status; `网关保活` only recognizes candidate accounts from `active`.
