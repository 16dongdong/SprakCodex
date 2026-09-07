# Несоответствия между текущим шлюзом и официальным Codex

Сохраняются только те различия в заголовках запросов, которые наиболее необходимы для продолжения обработки.

## `/v1/responses` Заголовок запроса

| Поля | Официальный Codex | Текущий шлюз | Текущая разница |
| --- | --- | --- | --- |
| `Authorization` | `Bearer <токен официального аккаунта>` | `Bearer <токен текущего аккаунта>` | Шлюз заменит токен аккаунта |
| `User-Agent` | `codex_cli_rs/<версия на этапе сборки> (<os/version; <arch>) <terminal>` | `codex_cli_rs/<версия из настройки базы данных> (<os/version; <arch>) <terminal>` | Официальный номер версии происходит от `env!("CARGO_PKG_VERSION")`, в настоящее время мы изменили его на настраиваемый базой данных; окончательное значение можно синхронизировать вручную, но источник несовместим |
| `x-client-request-id` | Фиксировано, равно `conversation_id` | Приоритет равен точке привязки потока | При переключении номеров и потоков он станет новой точкой привязки потока |
| `session_id` | Фиксировано, равно `conversation_id` | Приоритет равен привязке потока | Обычный `/responses` Больше не отправляется, если нет привязки темы |
| `x-codex-turn-state` | Воспроизведение в пределах одного хода | Воспроизведение, когда тот же поток стабилен | Будет активно отбрасываться при переключении номеров или замене резьбы |

## Текущий вывод

1. Наиболее ценными отличиями на данный момент являются эти 5 заголовков запросов/поведения транспортного уровня.
2. `gatewayOriginator` Значение настройки по-прежнему останется в локальной конфигурации, но оно больше не будет влиять на фактический исходящий `originator`. Фактический исходящий трафик фиксируется на официальном значении по умолчанию `codex_cli_rs`.
3. `User-Agent` Для номера версии официальным источником является версия пакета времени компиляции; Чтобы облегчить сопоставление с официальной версией вручную, текущий шлюз изменил ее на поле базы данных, которое можно настроить.

## Основа исходного кода

- Официальный `codex`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\client.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\requests\headers.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\default_client.rs`
- Текущий шлюз
  - [transport.rs](../../../crates/service/src/gateway/upstream/attempt_flow/transport.rs)
  - [codex_headers.rs](../../../crates/service/src/gateway/upstream/headers/codex_headers.rs)
  - [runtime_config.rs](../../../crates/service/src/gateway/core/runtime_config.rs)