#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:?usage: stage-linux-web.sh <release-dir> <arch> [stage-root]}"
arch="${2:?usage: stage-linux-web.sh <release-dir> <arch> [stage-root]}"
stage_root="${3:-stage}"

release_assets_dir="${release_dir}/release-assets"
service_zip="${stage_root}/CodexManager-service-linux-${arch}.zip"
web_zip="${release_assets_dir}/CodexManager-web-linux-${arch}.zip"

mkdir -p "$release_assets_dir"
test -f "$service_zip" || {
  echo "service package not found: $service_zip"
  exit 1
}

cp -f "$service_zip" "$web_zip"
echo "web package: $web_zip"
