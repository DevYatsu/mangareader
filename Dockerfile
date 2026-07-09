# Single-stage build — simpler, avoids Docker layer caching issues.
FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium chromium-driver ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p website && \
    cp target/release/website /app/mangaread-server && \
    rm -rf target

EXPOSE 3000
ENV CHROMIUM_FLAGS="--no-sandbox --disable-dev-shm-usage --disable-gpu"
CMD ["/app/mangaread-server"]
