# miden-testnet-bridge

Mock 1Click bridge between local Anvil and a local Miden node, shaped to match the NEAR Intents 1Click API cutover surface.

## Quickstart

1. Copy the example env file:

   ```bash
   cp .env.example .env
   ```

2. Start the scaffold stack:

   ```bash
   docker compose up -d
   ```

3. Check the bridge:

   ```bash
   curl -i http://localhost:8080/healthz
   ```

This PR only brings up `bridge` and `postgres`. Anvil lands in PR #5 and the local Miden node compose stack lands in PR #6.

For the Mac Studio local override, opt in explicitly:

```bash
docker compose -f compose.yaml -f compose.local.yml up -d
```

## Environment

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | `postgres://postgres:postgres@postgres:5432/miden_bridge` | Postgres DSN used by the bridge service. |
| `MIDEN_RPC_URL` | Yes | `http://localhost:57291` | Local Miden RPC endpoint placeholder for later PRs. |
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

