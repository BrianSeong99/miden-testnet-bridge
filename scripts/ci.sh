#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

run_ci() {
  export PATH="/usr/local/cargo/bin:${PATH}"
  if ! command -v docker >/dev/null 2>&1 && command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y --no-install-recommends docker-cli
  fi
  rustup component add clippy rustfmt 2>&1 | tail -1
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo build --locked
  cargo test --locked --lib --test evm --test hardening --test lifecycle --test miden_bridge --test miden_node --test state
}

docker_info_output=""
if docker_info_output="$(docker info 2>&1)"; then
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
    bash -lc "$(declare -f run_ci); run_ci"
else
  if grep -Eqi "cannot connect to the docker daemon|permission denied|no such file or directory" <<<"$docker_info_output"; then
    echo "WARNING: docker socket unreachable; falling back to host CI execution" >&2
    echo "$docker_info_output" >&2
    run_ci
  else
    echo "ERROR: docker info failed for a non-socket reason; refusing host fallback" >&2
    echo "$docker_info_output" >&2
    exit 1
  fi
fi
