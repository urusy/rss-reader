# RSS Reader — task runner. Run `just` to list commands.
# Requires: just, docker, cargo, node 22 + pnpm (corepack enable).

set dotenv-load := true
# Run recipes via bash so tools on the interactive PATH (e.g. pnpm at
# /opt/homebrew/bin) resolve like in the shell. The default `sh` does not.
set shell := ["bash", "-cu"]

default:
    @just --list

# Test hook: verifies the recipe shell can resolve pnpm (see scripts/test).
_pnpm-version:
    pnpm --version

# --- Local development ----------------------------------------------------

# Start only the database (for local cargo run / pnpm dev).
dev-db:
    docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d db

# Run the backend with auto-reload (needs: cargo install cargo-watch).
back:
    cd backend && cargo watch -x run

# Run the frontend dev server (Vite, proxies /api to :8080).
front:
    cd frontend && pnpm install && pnpm dev

# --- Build ----------------------------------------------------------------

build: build-back build-front

build-back:
    cd backend && cargo build --release

build-front:
    cd frontend && pnpm install && pnpm build

# --- Quality --------------------------------------------------------------

fmt:
    cd backend && cargo fmt
    cd frontend && pnpm exec prettier -w . || true

lint:
    cd backend && cargo clippy --all-targets -- -D warnings
    cd frontend && pnpm typecheck

test:
    cd backend && cargo test
    cd frontend && pnpm install && pnpm test

# --- Database migrations (needs: cargo install sqlx-cli) ------------------

migrate:
    cd backend && sqlx migrate run

migrate-add name:
    cd backend && sqlx migrate add {{name}}

# --- Docker (full stack) --------------------------------------------------

up:
    docker compose up -d --build

down:
    docker compose down

logs:
    docker compose logs -f

# Build images via script (honors REGISTRY / TAG from .env).
docker-build:
    ./scripts/build.sh

# Push images to the configured registry.
docker-push:
    ./scripts/push-images.sh
