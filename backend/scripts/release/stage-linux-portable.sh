#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:?usage: stage-linux-portable.sh <release-dir> <arch>}"
arch="${2:?usage: stage-linux-portable.sh <release-dir> <arch>}"
portable_dir="${release_dir}/portable"
portable_zip_name="CodexManager-linux-${arch}-portable.zip"

mkdir -p "$portable_dir"

source_bin="${release_dir}/CodexManager"
if [[ ! -f "$source_bin" ]]; then
  echo "portable source binary not found: $source_bin"
  exit 1
fi

portable_bin="${portable_dir}/CodexManager-portable"
cp -f "$source_bin" "$portable_bin"
chmod +x "$portable_bin"

zip_path="${portable_dir}/${portable_zip_name}"
rm -f "$zip_path"
(
  cd "$portable_dir"
  zip -q "$portable_zip_name" "CodexManager-portable"
)

echo "portable package: $zip_path"
