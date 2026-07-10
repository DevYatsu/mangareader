# ── Stage 1: Builder ──
# Installs build dependencies, fetches crate metadata, then compiles.
# Cargo dependency layers are cached so source-only changes rebuild fast.
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests first so cargo can fetch without seeing source.
COPY Cargo.toml Cargo.lock ./
COPY mangaread-core/Cargo.toml mangaread-core/
COPY scraper/Cargo.toml scraper/
COPY website/Cargo.toml website/

# Create placeholder src/main.rs so the workspace resolves (unused, overwritten later).
RUN mkdir -p mangaread-core/src scraper/src website/src && \
    echo "fn main() {}" > mangaread-core/src/lib.rs && \
    echo "fn main() {}" > scraper/src/lib.rs && \
    echo "fn main() {}" > website/src/main.rs && \
    cargo fetch --locked

# Now copy the real source and build.
COPY . .
RUN cargo build --release -p website && \
    cp target/release/website /app/mangaread-server

# ── Stage 2: Runtime ──
# Only Chromium, libs, and the binary — no Rust toolchain or build artifacts.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium chromium-driver ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

# Run as non-root for security
RUN groupadd -r mangaread && useradd -r -g mangaread -m -d /app mangaread

WORKDIR /app
COPY --from=builder --chown=mangaread:mangaread /app/mangaread-server ./

# Ensure the data directory exists and is writable by the mangaread user.
# Docker volumes are mounted as root by default, so we create it here
# and chown it while we still have root.
RUN mkdir -p /app/data && chown mangaread:mangaread /app/data

# Copy static assets into the runtime image
COPY website/static /app/static

USER mangaread
EXPOSE 3000
ENV CHROMIUM_FLAGS="--no-sandbox --disable-dev-shm-usage --disable-gpu"
ENV MANGAREAD_STATIC_DIR=/app/static
CMD ["/app/mangaread-server"]
