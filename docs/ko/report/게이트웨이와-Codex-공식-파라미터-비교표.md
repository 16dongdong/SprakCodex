# 현재 게이트웨이와 공식 Codex 간의 불일치

처리를 계속하는 데 가장 필요한 요청 헤더의 차이점만 유지됩니다.

## `/v1/responses` 요청 헤더

| 필드 | 공식 Codex | 현재 게이트웨이 | 현재 차이 |
| --- | --- | --- | --- |
| `Authorization` | `Bearer <공식 계정 토큰>` | `Bearer <현재 계정 토큰>` | 게이트웨이는 계정 토큰을 대체합니다 |
| `User-Agent` | `codex_cli_rs/<컴파일 시점 버전> (<os/version; <arch>) <terminal>` | `codex_cli_rs/<DB 설정 버전> (<os/version; <arch>) <terminal>` | 공식 버전 번호는 `env!("CARGO_PKG_VERSION")`에서 나오며 현재 이를 데이터베이스 구성 가능으로 변경했습니다. 최종 값을 수동으로 동기화할 수 있지만 소스가 일치하지 않습니다 |
| `x-client-request-id` | `conversation_id`과 동일하게 수정됨 | 스레드 앵커 포인트와 동일한 우선순위 | 숫자와 스레드를 전환하면 새로운 스레드 앵커 포인트가 됩니다 |
| `session_id` | `conversation_id`와 동일하게 수정됨 | 스레드 앵커와 동일한 우선순위 | 일반 `/responses` 스레드 앵커가 없으면 더 이상 전송되지 않습니다 |
| `x-codex-turn-state` | 같은 턴 내에서 재생 | 동일한 스레드가 안정적인 경우 재생 | 숫자를 바꾸거나 스레드를 교체할 때 적극적으로 폐기됩니다 |

## 현재 결론

1. 이제 가장 가치 있는 차이점은 5가지 요청 헤더/전송 계층 동작입니다.
2. `gatewayOriginator` 설정 값은 여전히 ​​로컬 구성에 유지되지만 실제 아웃바운드 `originator`에는 더 이상 영향을 미치지 않습니다. 실제 아웃바운드는 공식 기본값 `codex_cli_rs`으로 고정됩니다.
3. `User-Agent` 버전 번호의 경우 공식 소스는 컴파일 타임 패키지 버전입니다. 공식 버전과의 수동 매칭을 용이하게 하기 위해 현재 게이트웨이에서는 이를 구성할 수 있는 데이터베이스 필드로 변경했습니다.

## 소스코드 기반

- 공식 `codex`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\client.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\requests\headers.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\default_client.rs`
- 현재 게이트웨이
  - [transport.rs](../../../crates/service/src/gateway/upstream/attempt_flow/transport.rs)
  - [codex_headers.rs](../../../crates/service/src/gateway/upstream/headers/codex_headers.rs)
  - [runtime_config.rs](../../../crates/service/src/gateway/core/runtime_config.rs)