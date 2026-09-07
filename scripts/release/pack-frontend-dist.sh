#!/usr/bin/env bash
set -euo pipefail

apps_dir="${1:-apps}"
artifact_path="${2:-frontend-dist.tar.gz}"

test -f "${apps_dir}/out/index.html" || {
  echo "${apps_dir}/out/index.html not found"
  exit 1
}

rm -f "$artifact_path"
tar -czf "$artifact_path" -C "$apps_dir" out
echo "frontend dist artifact: $artifact_path"
