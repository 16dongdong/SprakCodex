#!/usr/bin/env bash
set -euo pipefail

target="${1:-}"

args=(build --release --locked)
if [ -n "$target" ]; then
  args+=(--target "$target")
fi

args+=(
  -p codexmanager-service
  -p codexmanager-web
  -p codexmanager-start
  --features codexmanager-web/embedded-ui
)

cargo "${args[@]}"
