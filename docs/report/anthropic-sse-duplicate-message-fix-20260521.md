# Anthropic SSE 重复消息修复（2026-05-21）

## 背景

用户反馈 Claude Code 通过 `http://ip:port/` 接入 Anthropic Messages 协议时，终端里同一条助手文本会出现两次。

## 根因

`/v1/messages` 会被改写为 `/v1/responses`，再由 `AnthropicSseReader` 转回 Anthropic SSE。上游在工具调用链路里可能按以下顺序返回：

1. `response.output_text.delta`：文本增量
2. `response.output_item.done`：工具调用快照，本地会关闭当前文本 block
3. `response.completed`：包含完整输出文本的终态快照

旧逻辑用 `text_block_index.is_none()` 判断是否需要从 completed 快照补发文本。工具调用会关闭文本 block 并清空 `text_block_index`，因此 completed 全量文本会被再次作为 `content_block_delta` 发给 Claude Code，造成重复显示。

## 修复

- 在 `AnthropicSseState` 增加 `emitted_text_to_client`，单独记录是否已向客户端发送过文本。
- `response.completed` 只在从未发送过文本时补发快照，避免工具调用关闭文本 block 后重复回放。
- 新增回归测试覆盖“文本 delta → 工具调用 → completed 全量快照”链路，并确认日志用 `output_text` 不重复。

## 涉及文件

- `crates/service/src/gateway/observability/http_bridge/stream_readers/anthropic.rs`
- `crates/service/src/gateway/observability/tests/http_bridge_tests.rs`

## 验证

- `cargo test -p codexmanager-service anthropic_sse_reader_does_not_replay_completed_snapshot_after_tool_call`
- `cargo test -p codexmanager-service anthropic`
- `cargo test -p codexmanager-service gateway::http_bridge::tests`
