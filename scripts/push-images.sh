#!/usr/bin/env bash
# Push previously built images to the configured registry.
# Usage: ./scripts/push-images.sh   (REGISTRY/TAG は環境変数で上書き可)
#
# push 先 (REGISTRY) はこのスクリプトの既定に持たせる。アプリ実行設定の .env は
# 読み込まない（.env はアプリ用。ビルド/配布の設定と混ぜない）。
# 事前に `docker login -u urusy7` でログインしておくこと。
set -euo pipefail

cd "$(dirname "$0")/.."

# Docker Hub のユーザー名を既定 REGISTRY にする（Falcon と揃えて urusy7）。
REGISTRY="${REGISTRY:-urusy7}"
TAG="${TAG:-latest}"

if [ -z "${REGISTRY}" ] || [ "${REGISTRY}" = "localhost" ]; then
  echo "REGISTRY が未設定（または localhost）です。環境変数 REGISTRY を指定してください（例: REGISTRY=urusy7）。" >&2
  exit 1
fi

for name in rss-reader-backend rss-reader-frontend; do
  image="${REGISTRY}/${name}:${TAG}"
  echo "==> Pushing ${image}"
  docker push "${image}"
done

echo "==> Push complete."
