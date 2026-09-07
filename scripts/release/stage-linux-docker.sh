#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:?usage: stage-linux-docker.sh <release-dir>}"
docker_stage="${release_dir}/docker-release"
release_assets_dir="${release_dir}/release-assets"

rm -rf "$docker_stage"
mkdir -p "$docker_stage" "$release_assets_dir"

cp docker/Dockerfile.service.release "$docker_stage/Dockerfile.service"
cp docker/Dockerfile.web.release "$docker_stage/Dockerfile.web"
cp docker/docker-compose.release.yml "$docker_stage/docker-compose.yml"

for binary in codexmanager-service codexmanager-web codexmanager-start; do
  source="${release_dir}/${binary}"
  test -f "$source" || {
    echo "binary not found: $source"
    exit 1
  }
  cp "$source" "$docker_stage/$binary"
done

chmod +x "$docker_stage"/codexmanager-*

docker build -f "$docker_stage/Dockerfile.service" -t codexmanager-service:release "$docker_stage"
docker build -f "$docker_stage/Dockerfile.web" -t codexmanager-web:release "$docker_stage"

docker save codexmanager-service:release codexmanager-web:release | gzip -c > "$release_assets_dir/codexmanager-docker-images.tar.gz"
cp "$docker_stage/docker-compose.yml" "$release_assets_dir/docker-compose.release.yml"

echo "docker image archive: ${release_assets_dir}/codexmanager-docker-images.tar.gz"
echo "docker compose file: ${release_assets_dir}/docker-compose.release.yml"
