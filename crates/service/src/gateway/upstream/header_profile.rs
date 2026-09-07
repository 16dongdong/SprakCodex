#[path = "headers/mod.rs"]
mod headers_impl;

pub(crate) use headers_impl::{
    apply_codex_target_accept_header, build_codex_compact_upstream_headers,
    build_codex_upstream_headers, derive_sticky_conversation_id_from_headers,
    resolve_codex_installation_id, CodexCompactUpstreamHeaderInput, CodexUpstreamHeaderInput,
};
