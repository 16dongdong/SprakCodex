#!/usr/bin/env bash
set -euo pipefail

assets_dir="${1:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag> [auto|true|false]}"
repository_owner="${2:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag> [auto|true|false]}"
tag="${3:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag> [auto|true|false]}"
prerelease_input="${4:-auto}"

image_archive="${assets_dir}/codexmanager-docker-images.tar.gz"
test -f "$image_archive" || {
  echo "docker image archive not found: $image_archive"
  exit 1
}

gunzip -c "$image_archive" | docker load

owner="$(printf '%s' "$repository_owner" | tr '[:upper:]' '[:lower:]')"

publish_tags=("${tag}")
case "${prerelease_input}" in
  true)
    prerelease=true
    ;;
  false)
    prerelease=false
    ;;
  auto)
    if [[ "${tag}" == *-* ]]; then
      prerelease=true
    else
      prerelease=false
    fi
    ;;
  *)
    echo "invalid prerelease input: ${prerelease_input}" >&2
    exit 1
    ;;
esac

if [[ "${prerelease}" == "false" ]]; then
  publish_tags+=("stable" "latest")
fi

for publish_tag in "${publish_tags[@]}"; do
  docker tag codexmanager-service:release "ghcr.io/${owner}/codexmanager-service:${publish_tag}"
  docker tag codexmanager-web:release "ghcr.io/${owner}/codexmanager-web:${publish_tag}"

  docker push "ghcr.io/${owner}/codexmanager-service:${publish_tag}"
  docker push "ghcr.io/${owner}/codexmanager-web:${publish_tag}"
done
