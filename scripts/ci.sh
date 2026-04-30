#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker volume create miden-bridge-cargo-registry >/dev/null
docker volume create miden-bridge-target >/dev/null
docker run --rm \
  -v "$(pwd)":/work \
  -v miden-bridge-cargo-registry:/usr/local/cargo/registry \
  -v miden-bridge-target:/work/target \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -e DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}" \
  -w /work \
  rust:1.93-slim \
  bash -c "
    set -e
    rustup component add clippy rustfmt 2>&1 | tail -1
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo build --locked
    cargo test --locked
  "
