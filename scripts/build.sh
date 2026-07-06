#!/usr/bin/env bash
# Build backend & frontend Docker images.
# Usage: ./scripts/build.sh   (REGISTRY/TAG/PLATFORM は環境変数で上書き可)
#
# push 先 (REGISTRY) はこのスクリプトの既定に持たせる。アプリ実行設定の .env は
# 読み込まない（.env はアプリ用。ビルド/配布の設定と混ぜない）。
#
# 本番は x86 (linux/amd64)。開発機が Apple Silicon (arm64) でも本番で動く
# イメージを焼くため、PLATFORM 既定を linux/amd64 に固定する（QEMU 経由でビルド）。
# 上書きしたい場合は環境変数 PLATFORM を指定する（例: PLATFORM=linux/arm64）。
set -euo pipefail

cd "$(dirname "$0")/.."

# Docker Hub のユーザー名を既定 REGISTRY にする（Falcon と揃えて urusy7）。
REGISTRY="${REGISTRY:-urusy7}"
TAG="${TAG:-latest}"
PLATFORM="${PLATFORM:-linux/amd64}"

BACKEND_IMAGE="${REGISTRY}/rss-reader-backend:${TAG}"
FRONTEND_IMAGE="${REGISTRY}/rss-reader-frontend:${TAG}"

echo "==> Building ${BACKEND_IMAGE} (${PLATFORM})"
docker build --platform "${PLATFORM}" -t "${BACKEND_IMAGE}" ./backend

echo "==> Building ${FRONTEND_IMAGE} (${PLATFORM})"
docker build --platform "${PLATFORM}" -t "${FRONTEND_IMAGE}" ./frontend

echo "==> Done."
echo "    ${BACKEND_IMAGE}"
echo "    ${FRONTEND_IMAGE}"
