# syntax=docker/dockerfile:1

# --- Dependency planning (cargo-chef) -------------------------------------
FROM rust:1.82-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Build ----------------------------------------------------------------
FROM chef AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
# Cache dependency compilation; only re-runs when Cargo.toml/lock change.
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin rss-reader-backend

# --- Runtime --------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 appuser
WORKDIR /app
COPY --from=builder /app/target/release/rss-reader-backend /usr/local/bin/app
USER appuser
EXPOSE 8080
ENV BIND_ADDR=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/app"]
