# ── Build stage ──
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY mangaread-core/Cargo.toml mangaread-core/
COPY scraper/Cargo.toml scraper/
COPY website/Cargo.toml website/

# Build dependencies first (cached layer)
RUN mkdir mangaread-core/src scraper/src website/src && \
    echo "fn main() {}" > website/src/main.rs && \
    echo "fn main() {}" > scraper/src/main.rs && \
    echo "// placeholder" > mangaread-core/src/lib.rs && \
    echo "// placeholder" > scraper/src/lib.rs && \
    cargo build --release -p website 2>/dev/null || true

# Now build the real thing
COPY . .
RUN cargo build --release -p website

# ── Runtime stage ──
FROM debian:bookworm-slim

# Install Chrome + dependencies for the scraper
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    chromium-driver \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Chromium needs --no-sandbox in Docker
ENV CHROMIUM_FLAGS="--no-sandbox --disable-dev-shm-usage --disable-gpu"

WORKDIR /app
COPY --from=builder /app/target/release/website /app/website

EXPOSE 3000

CMD ["/app/website"]
