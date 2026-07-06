#!/usr/bin/env bash
# ===========================================
# rss-reader - Docker Image Build & Push Script
# ===========================================
# Usage:
#   ./scripts/docker-build-push.sh [options]
#
# Options:
#   --all            Build and push all images (default)
#   --frontend       Build and push frontend image only
#   --backend        Build and push backend image only
#   --tag <tag>      Use specific tag (default: value of TAG or 'latest')
#   --no-push        Build only, don't push to registry
#   --help, -h       Show this help message
#
# Registry / tag / platform は環境変数で上書き可（REGISTRY / TAG / PLATFORM）、
# scripts/build.sh & push-images.sh と揃える。push 先の既定はこのスクリプトに持たせ、
# アプリ実行設定の .env は読み込まない（.env はアプリ用。ビルド/配布設定と混ぜない）。
# 本番は x86 (linux/amd64)。開発機が Apple Silicon でも本番で動くイメージを
# 焼くため PLATFORM 既定を linux/amd64 に固定する（QEMU 経由でビルド）。
#
# Examples:
#   ./scripts/docker-build-push.sh                      # Build and push all
#   ./scripts/docker-build-push.sh --backend --frontend # Build specific images
#   ./scripts/docker-build-push.sh --tag v1.0.0         # Use specific tag
#   ./scripts/docker-build-push.sh --no-push            # Build only
# ===========================================

set -euo pipefail

# BuildKit を有効化してキャッシュマウント（--mount=type=cache）を利用可能にする
export DOCKER_BUILDKIT=1

# Record start time
START_TIME=$(date +%s)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Registry settings（環境変数で上書き可）。アプリ用 .env は読まない。
# Docker Hub のユーザー名を既定 REGISTRY にする（Falcon と揃えて urusy7）。
REGISTRY="${REGISTRY:-urusy7}"
IMAGE_TAG="${TAG:-latest}"
PLATFORM="${PLATFORM:-linux/amd64}"

# Build flags
BUILD_BACKEND=false
BUILD_FRONTEND=false
PUSH_IMAGES=true

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --all)
            BUILD_BACKEND=true
            BUILD_FRONTEND=true
            shift
            ;;
        --backend)
            BUILD_BACKEND=true
            shift
            ;;
        --frontend)
            BUILD_FRONTEND=true
            shift
            ;;
        --tag)
            IMAGE_TAG="$2"
            shift 2
            ;;
        --no-push)
            PUSH_IMAGES=false
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --all              Build and push all images (default)"
            echo "  --frontend         Build and push frontend image only"
            echo "  --backend          Build and push backend image only"
            echo "  --tag <tag>        Use specific tag (default: value of TAG or 'latest')"
            echo "  --no-push          Build only, don't push to registry"
            echo "  --help, -h         Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown argument: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# If no specific image selected, build all
if [ "$BUILD_BACKEND" = false ] && [ "$BUILD_FRONTEND" = false ]; then
    BUILD_BACKEND=true
    BUILD_FRONTEND=true
fi

# Guard: refuse to push to localhost (same policy as push-images.sh)
if [ "$PUSH_IMAGES" = true ] && [ "$REGISTRY" = "localhost" ]; then
    echo -e "${RED}REGISTRY is 'localhost'. Set REGISTRY in .env (e.g. ghcr.io/you) before pushing,${NC}"
    echo -e "${RED}or run with --no-push to build only.${NC}"
    exit 1
fi

echo -e "${BLUE}===========================================${NC}"
echo -e "${BLUE}  rss-reader Docker Build & Push${NC}"
echo -e "${BLUE}===========================================${NC}"
echo ""
echo -e "Registry: ${YELLOW}${REGISTRY}${NC}"
echo -e "Platform: ${YELLOW}${PLATFORM}${NC}"
echo -e "Tag:      ${YELLOW}${IMAGE_TAG}${NC}"
echo -e "Push:     ${YELLOW}${PUSH_IMAGES}${NC}"
echo ""

# Temporary directory for build logs
LOG_DIR=$(mktemp -d)
trap 'rm -rf "$LOG_DIR"' EXIT

# Arrays to track background jobs
declare -a BUILD_PIDS
declare -a BUILD_NAMES
declare -a BUILD_IMAGES

# Function to build an image in background
build_image() {
    local name=$1
    local image=$2
    local context=$3
    local log_file="${LOG_DIR}/${name}.log"

    echo -e "${BLUE}[Building] ${name}...${NC}"
    if docker build --platform "${PLATFORM}" -t "${image}" "${context}" > "${log_file}" 2>&1; then
        echo -e "${GREEN}✓ ${name} built${NC}"
        return 0
    else
        echo -e "${RED}✗ ${name} build failed${NC}"
        echo -e "${YELLOW}Log output:${NC}"
        tail -50 "${log_file}"
        return 1
    fi
}

# Function to push an image in background
push_image() {
    local name=$1
    local image=$2
    local log_file="${LOG_DIR}/${name}-push.log"

    if docker push "${image}" > "${log_file}" 2>&1; then
        echo -e "${GREEN}✓ ${name} pushed${NC}"
        return 0
    else
        echo -e "${RED}✗ ${name} push failed${NC}"
        tail -20 "${log_file}"
        return 1
    fi
}

# Start parallel builds
echo -e "${BLUE}=== Starting parallel builds ===${NC}"
echo ""

if [ "$BUILD_BACKEND" = true ]; then
    build_image "backend" "${REGISTRY}/rss-reader-backend:${IMAGE_TAG}" "${PROJECT_ROOT}/backend" &
    BUILD_PIDS+=($!)
    BUILD_NAMES+=("backend")
    BUILD_IMAGES+=("${REGISTRY}/rss-reader-backend:${IMAGE_TAG}")
fi

if [ "$BUILD_FRONTEND" = true ]; then
    build_image "frontend" "${REGISTRY}/rss-reader-frontend:${IMAGE_TAG}" "${PROJECT_ROOT}/frontend" &
    BUILD_PIDS+=($!)
    BUILD_NAMES+=("frontend")
    BUILD_IMAGES+=("${REGISTRY}/rss-reader-frontend:${IMAGE_TAG}")
fi

# Wait for all builds to complete and check results
BUILD_FAILED=false
for i in "${!BUILD_PIDS[@]}"; do
    if ! wait "${BUILD_PIDS[$i]}"; then
        BUILD_FAILED=true
        echo -e "${RED}Build failed for ${BUILD_NAMES[$i]}${NC}"
    fi
done

if [ "$BUILD_FAILED" = true ]; then
    echo -e "${RED}One or more builds failed. Aborting.${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}=== All builds completed ===${NC}"
echo ""

# Push images (also in parallel)
if [ "$PUSH_IMAGES" = true ]; then
    echo -e "${BLUE}=== Starting parallel pushes ===${NC}"
    echo ""

    declare -a PUSH_PIDS

    for i in "${!BUILD_NAMES[@]}"; do
        push_image "${BUILD_NAMES[$i]}" "${BUILD_IMAGES[$i]}" &
        PUSH_PIDS+=($!)
    done

    # Wait for all pushes to complete
    PUSH_FAILED=false
    for i in "${!PUSH_PIDS[@]}"; do
        if ! wait "${PUSH_PIDS[$i]}"; then
            PUSH_FAILED=true
        fi
    done

    if [ "$PUSH_FAILED" = true ]; then
        echo -e "${RED}One or more pushes failed.${NC}"
        exit 1
    fi

    echo ""
    echo -e "${GREEN}=== All pushes completed ===${NC}"
fi
echo ""

# Calculate elapsed time
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
MINUTES=$((ELAPSED / 60))
SECS=$((ELAPSED % 60))

echo -e "${BLUE}===========================================${NC}"
echo -e "${GREEN}Done!${NC}"
echo -e "Elapsed time: ${YELLOW}${MINUTES}m ${SECS}s${NC}"
echo -e "${BLUE}===========================================${NC}"
