# miden-testnet-bridge

`miden-testnet-bridge` is a local 1Click-compatible bridge shim for wallet and consumer integration work: it exposes the same `/v0/*` surface the wallet team expects, watches deposits on local Anvil and a local Miden node, and settles or refunds quotes through the bridge lifecycle. It is temporary infrastructure with a planned 6-week service life, intended to unblock integration work until the real NEAR 1Click service cutover in W7, with a target date of 2026-06-12.

## Quickstart

Fresh clone to working local stack:

```bash
cp .env.example .env
make genesis           # one-time, captures genesis account ID
docker compose up -d   # bridge + postgres + anvil + miden-node
curl http://localhost:8080/healthz
```

Expected result:

```text
ok
```

`make genesis` is idempotent. After the first run, the Docker volumes keep the Miden chain state, bootstrap accounts, bridge-side Miden store, Anvil state, and Postgres data.

For the Mac Studio homelab override, opt in explicitly:

```bash
docker compose -f compose.yaml -f compose.local.yml up -d
```

## What Runs

- `bridge`: Axum HTTP service on `http://localhost:8080`
- `postgres`: quote state, lifecycle events, chain artifacts, Miden bootstrap records
- `anvil`: local EVM chain on `http://localhost:8545`
- `anvil-init`: one-shot token/bootstrap funding job
- `miden-node`: local Miden RPC on `http://localhost:57291`

## Cutover

When NEAR’s real 1Click endpoint goes live, consumers do not change paths or payloads. They flip one environment variable to the new base URL:

- Base URL to use: `https://1click.chaindefuser.com`
- Do not include `/v0` in the env var value
- Clients keep appending paths such as `/v0/tokens`, `/v0/quote`, and `/v0/status`

Wallet-team cutover steps:

1. Find the consumer-side env var or config entry that currently points at this bridge base URL, for example `http://localhost:8080`.
2. Replace only the base URL value with `https://1click.chaindefuser.com`.
3. Leave request code unchanged so the client still calls path-suffixed routes like `/v0/tokens`.
4. Restart or redeploy the consumer so the new base URL is loaded.
5. Verify the swap took effect with a `/v0/tokens` round-trip.

Verification:

```bash
export ONECLICK_BASE_URL="https://1click.chaindefuser.com"
curl -fsS "${ONECLICK_BASE_URL}/v0/tokens" | jq '.[0]'
```

If the env var is set to `https://1click.chaindefuser.com/v0`, clients that append `/v0/tokens` will incorrectly request `/v0/v0/tokens`. The env var must hold the base URL only.

Sunset and consumer migration details live in [docs/SUNSET.md](docs/SUNSET.md).

## Environment

Copy `.env.example` to `.env`, then override only what you need:

```bash
cp .env.example .env
```

The "Default" column is what the binary falls back to when the variable is unset. `.env.example` ships with developer-friendly localhost values for the host-mode dev path; container deployments rely on the binary fallbacks.

| Variable | Default | Description |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://postgres:postgres@postgres:5432/miden_bridge` | Postgres DSN used by the bridge container. |
| `MIDEN_RPC_URL` | `http://localhost:57291` | Miden RPC URL for host-side runs; Compose overrides this to `http://miden-node:57291` inside the bridge container. |
| `MIDEN_STORE_DIR` | `./.miden-store` | Host-run Miden client store path; Compose overrides this to `/var/lib/bridge/miden-store`. |
| `MIDEN_MASTER_SEED_HEX` | `0101010101010101010101010101010101010101010101010101010101010101` | 32-byte hex seed used to derive deterministic Miden accounts for bootstrap and outbound deposit accounts. |
| `EVM_RPC_URL` | `http://host.docker.internal:8545` | Binary fallback for reaching a host-run Anvil node from containerized bridge runs; `.env.example` uses `http://localhost:8545` for host-mode development. |
| `MASTER_MNEMONIC` | `abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about` | Deterministic master mnemonic used to derive per-quote EVM deposit addresses. |
| `SOLVER_PRIVATE_KEY` | `replace-with-solver-private-key` | Binary fallback placeholder; set this explicitly in real runs. `.env.example` uses a local Anvil key for host-mode development. |
| `EVM_CHAIN_ID` | `271828` | Chain ID expected from the local Anvil instance. |
| `EVM_TOKEN_ADDRESSES_PATH` | `/state/token-addresses.json` | JSON file written by `anvil-init` that maps mock ERC-20 symbols to deployed token addresses. |
| `BRIDGE_HTTP_PORT` | `8080` | Host port exposed by the bridge HTTP server. |
| `RUST_LOG` | `info,sqlx=warn,hyper=warn,tower_http=warn` | Tracing filter for the bridge process. |
| `LOG_FORMAT` | `json` | Bridge log output format: `json` or `pretty`. |
| `DEADLINE_SCAN_INTERVAL_SECS` | `30` | Poll interval for the deadline-expiry scanner and stuck-`PROCESSING` watchdog. |

## Local CI

`bash scripts/ci.sh` is the merge gate for this repo. Run it before review and before push:

```bash
bash scripts/ci.sh
```

It runs:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --locked`
- `cargo test --locked`

## E2E Tests

Run the end-to-end suite against a live local stack:

```bash
make genesis
make e2e
```

`make e2e` runs:

```bash
RUN_E2E=1 cargo test --test e2e -- --test-threads=1
```

The `--test-threads=1` requirement is intentional. The E2E suite shares Docker services and persistent chain state, so it must run serially.

## Operations

- Runbook: [docs/RUNBOOK.md](docs/RUNBOOK.md)
- Sunset and cutover plan: [docs/SUNSET.md](docs/SUNSET.md)
- Architecture diagrams: [docs/architecture.md](docs/architecture.md)
- OpenAPI snapshot: [docs/openapi.yaml](docs/openapi.yaml)

## Milestone

- GitHub milestone: <https://github.com/BrianSeong99/miden-testnet-bridge/milestone/1>
