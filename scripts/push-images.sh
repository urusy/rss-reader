#!/usr/bin/env bash
# Push previously built images to the configured registry.
# Usage: ./scripts/push-images.sh
set -euo pipefail

cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a

REGISTRY="${REGISTRY:-localhost}"
TAG="${TAG:-latest}"

if [ "${REGISTRY}" = "localhost" ]; then
  echo "REGISTRY is 'localhost'. Set REGISTRY in .env (e.g. ghcr.io/you) before pushing." >&2
  exit 1
fi

for name in rss-reader-backend rss-reader-frontend; do
  image="${REGISTRY}/${name}:${TAG}"
  echo "==> Pushing ${image}"
  docker push "${image}"
done

echo "==> Push complete."
