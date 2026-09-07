#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:?usage: rebuild-macos-dmg.sh <release-dir> <arch> [tauri-config-path]}"
arch="${2:?usage: rebuild-macos-dmg.sh <release-dir> <arch> [tauri-config-path]}"
tauri_config_path="${3:-apps/src-tauri/tauri.conf.json}"

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

dmg_dir="${bundle_root}/dmg"
mkdir -p "$dmg_dir"
find "$dmg_dir" -type f -name '*.dmg' -delete

if [ "$arch" = "arm64" ]; then
  dmg_arch="aarch64"
else
  dmg_arch="x64"
fi

version="$(
  python3 -c 'import json, pathlib, sys; print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["version"])' "$tauri_config_path"
)"

stage_dir="$(mktemp -d)"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$stage_dir" "$temp_dir"' EXIT

ditto "$bundle_dir" "$stage_dir/CodexManager.app"
ln -s /Applications "$stage_dir/Applications"
install -m 0755 "assets/macos/Open CodexManager.command" "$stage_dir/Open CodexManager.command"
install -m 0644 "assets/macos/README-macOS-first-launch.txt" "$stage_dir/README-macOS-first-launch.txt"

codesign --force --deep --sign - "$stage_dir/CodexManager.app"
codesign --verify --deep --strict "$stage_dir/CodexManager.app"

dmg_path="${dmg_dir}/CodexManager_${version}_${dmg_arch}.dmg"
temp_dmg_path="${temp_dir}/CodexManager_${version}_${dmg_arch}.dmg"
created=0

for attempt in 1 2 3; do
  rm -f "$temp_dmg_path"
  hdiutil detach "/Volumes/CodexManager" -force >/dev/null 2>&1 || true
  sync || true
  sleep "$attempt"
  if hdiutil create -volname "CodexManager" -srcfolder "$stage_dir" -ov -format UDZO "$temp_dmg_path"; then
    created=1
    break
  fi
  echo "hdiutil create failed on attempt ${attempt}, retrying..."
done

if [ "$created" -ne 1 ]; then
  echo "macOS dmg create failed after retries"
  exit 1
fi

mv -f "$temp_dmg_path" "$dmg_path"
test -f "$dmg_path" || {
  echo "macOS dmg not created: $dmg_path"
  exit 1
}

echo "macOS dmg: $dmg_path"
