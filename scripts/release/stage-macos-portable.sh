#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:?usage: stage-macos-portable.sh <release-dir>}"
bundle_root="${release_dir}/bundle"

test -d "$bundle_root" || {
  echo "macOS bundle root not found: $bundle_root"
  exit 1
}

bundle_dir="$(find "$bundle_root" -type d -path '*/CodexManager.app' | head -n 1)"
test -n "$bundle_dir" || {
  echo "macOS app bundle not found under: $bundle_root"
  exit 1
}

portable_dir="${release_dir}/portable"
mkdir -p "$portable_dir"

zip_path="${portable_dir}/CodexManager-macos-portable.zip"
rm -f "$zip_path"
ditto -c -k --sequesterRsrc --keepParent "$bundle_dir" "$zip_path"

echo "legacy portable zip: $zip_path"
echo "source app bundle: $bundle_dir"
