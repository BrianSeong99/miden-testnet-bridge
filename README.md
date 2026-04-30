# miden-testnet-bridge

Mock 1Click bridge between local Anvil and a local Miden node, shaped to match the NEAR Intents 1Click API cutover surface.

## Quickstart

1. Bootstrap the local Miden node once:

   ```bash
   make genesis
   ```

2. Start the scaffold stack:

   ```bash
   docker compose up -d --build
   ```

3. Check the bridge:

   ```bash
   curl -i http://localhost:8080/healthz
   ```

The Miden bootstrap is idempotent. After the first run, `docker compose down && docker compose up -d` reuses the named `miden-node-data`, `miden-node-accounts`, and `bridge-miden-store` volumes.

For the Mac Studio local override, opt in explicitly:

```bash
docker compose -f compose.yaml -f compose.local.yml up -d
```

## Environment

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | `postgres://postgres:postgres@postgres:5432/miden_bridge` | Postgres DSN used by the bridge service. |
| `MIDEN_RPC_URL` | Yes | `http://miden-node:57291` in Compose, `http://localhost:57291` for host runs | Local Miden RPC endpoint. |
| `MIDEN_STORE_DIR` | Yes | `/var/lib/bridge/miden-store` in Compose, `./.miden-store` for host runs | Persistent SQLite store + keystore for the Rust Miden client. |
| `EVM_RPC_URL` | Yes | `http://localhost:8545` | Local Anvil RPC endpoint placeholder for later PRs. |
| `MASTER_MNEMONIC` | Yes | `replace-with-master-mnemonic` | Seed material for deterministic quote wallet derivation in later PRs. |
| `SOLVER_PRIVATE_KEY` | Yes | `replace-with-solver-private-key` | Solver-side EVM key placeholder for later PRs. |
| `BRIDGE_HTTP_PORT` | No | `8080` | Port exposed by the bridge service. |
| `RUST_LOG` | No | `info` | Tracing filter. |
| `LOG_FORMAT` | No | `json` | `json` or `pretty`. |

## Local CI

Run the local Docker gate before opening or updating a PR:

```bash
bash scripts/ci.sh
```

## Miden Node Bootstrap

`make genesis` runs `miden-node bundled bootstrap` inside Docker and writes the chain state into named Docker volumes. The bridge container does not report healthy until both Postgres and the local Miden RPC are reachable.
