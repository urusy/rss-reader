#!/usr/bin/env bash
# Build backend & frontend Docker images.
# Usage: ./scripts/build.sh   (reads REGISTRY/TAG from .env or environment)
set -euo pipefail

cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a

REGISTRY="${REGISTRY:-localhost}"
TAG="${TAG:-latest}"

BACKEND_IMAGE="${REGISTRY}/rss-reader-backend:${TAG}"
FRONTEND_IMAGE="${REGISTRY}/rss-reader-frontend:${TAG}"

echo "==> Building ${BACKEND_IMAGE}"
docker build -t "${BACKEND_IMAGE}" ./backend

echo "==> Building ${FRONTEND_IMAGE}"
docker build -t "${FRONTEND_IMAGE}" ./frontend

echo "==> Done."
echo "    ${BACKEND_IMAGE}"
echo "    ${FRONTEND_IMAGE}"
