FROM rust:1.93-slim-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system bridge \
    && useradd --system --gid bridge --create-home bridge

WORKDIR /app
COPY --from=builder /app/target/release/miden-testnet-bridge /usr/local/bin/miden-testnet-bridge
COPY migrations ./migrations

ENV BRIDGE_HTTP_PORT=8080 \
    LOG_FORMAT=json \
    RUST_LOG=info

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${BRIDGE_HTTP_PORT}/healthz" >/dev/null || exit 1

USER bridge
CMD ["miden-testnet-bridge"]
