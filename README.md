# miden-testnet-bridge

Mock **NEAR Intents 1Click** bridge between an EVM chain and Miden testnet.

This repo is a builder sandbox: third-party app teams can run a local mock of
the NEAR Intents 1Click API, point their app at it, and test the same
quote/deposit/status flow they would use against the hosted 1Click service. The
default runnable profile is public Miden testnet plus local Anvil.

This is testnet-only infrastructure for local testing, Anvil, Sepolia, and
public Miden testnet. It is not a production bridge, not a mainnet integration
path, and must not be used with mainnet funds.

The validated path in this repo is public Miden testnet plus local Anvil. Local
Miden node mode still exists as a manual fallback, but it is not the acceptance
path for this bridge pivot.

## Builder Quick Start

For a step-by-step testnet walkthrough aimed at third-party builders and
automation agents, start with
[`docs/builder-testing-guide.md`](docs/builder-testing-guide.md).

```bash
git clone https://github.com/BrianSeong99/miden-testnet-bridge.git
cd miden-testnet-bridge
cp .env.anvil.example .env
make sandbox
```

Open:

```text
http://localhost:8080/lab
```

Monitor from the CLI:

```bash
./bin/bridgectl status
./bin/bridgectl tokens
./bin/bridgectl flows
./bin/bridgectl flow <correlation-id>
make sandbox-logs
make sandbox-reset
```

The primary integration surface remains NEAR Intents 1Click-shaped:

```text
GET  /v0/tokens
POST /v0/quote
POST /v0/deposit/submit
GET  /v0/status
```

The `/demo/*` endpoints and `/lab` UI are sandbox helpers. They exist so a
builder can click through Anvil/Miden transfers and inspect artifacts; app
integrations should use the `/v0/*` API.

## What This Proves

- Inbound: an Anvil deposit is detected, then the Bridge API submits a
  solver-signed public P2ID note on Miden testnet for the recipient.
- Outbound: a user creates a public programmable `BridgeOutV1` note on Miden
  testnet, the bridge consumes that note, then releases funds on Anvil.
- Restart recovery: a quote in `PROCESSING` survives a bridge container restart
  and resumes from durable transaction ids.
- Evidence logging: E2E runs print correlation ids, Miden tx ids, EVM tx hashes,
  and lifecycle evidence.

Sepolia native ETH is also live-validated for both directions. The 2026-05-15
run used public Miden testnet plus Sepolia through
`https://gateway.tenderly.co/public/sepolia`, with public Sepolia tx hashes and
Miden tx ids recorded in `docs/smoke-test-report.html`.

## Bridge Shape

```mermaid
flowchart LR
    User["User / solver client"] --> BridgeAPI["Bridge API"]
    BridgeAPI --> Postgres["Postgres quote state"]
    BridgeAPI --> Anvil["Anvil EVM"]
    BridgeAPI --> Miden["Miden testnet via miden-client"]
    Miden --> Notes["Public Miden notes"]
    Notes --> Claim["Recipient or bridge consumes note"]
    Claim --> Final["Release, refund, or wallet balance update"]
```

The mock intentionally follows the NEAR Intents 1Click lifecycle:

1. Builder app fetches supported assets from `/v0/tokens`.
2. Builder app requests a quote from `/v0/quote`.
3. User sends the origin-chain deposit to the returned deposit address or
   creates the returned Miden public-note deposit.
4. Builder app may notify the mock with `/v0/deposit/submit`.
5. Builder app polls `/v0/status` until `SUCCESS`, `REFUNDED`, or `FAILED`.

Inbound completion has two layers:

1. The bridge marks the quote `SUCCESS` after the solver-signed public P2ID note
   is committed and consumable on Miden.
2. The recipient completes wallet-side settlement by syncing and consuming that
   public P2ID note.

Outbound uses public programmable notes because Miden accounts are not reliably
discoverable before they have sent a transaction. The bridge therefore watches
for a public `BridgeOutV1` note targeted to the stable bridge account instead of
deriving one deposit account per quote.

## Prerequisites

- Docker with Compose v2.
- Rust toolchain compatible with edition 2024. The Docker CI path uses
  `rust:1.93-slim`.
- OpenSSL for the seed-generation helper used below.
- Network access to `https://rpc.testnet.miden.io`.
- Enough time for public testnet bootstrap and confirmations. A full serialized
  E2E run took about 14 minutes in the recorded evidence run.

No local Miden node is required for the supported path. The bridge uses
`miden-client` network defaults for Miden testnet, including the native remote
transaction prover configuration.

## Clone And Start

```bash
git clone https://github.com/BrianSeong99/miden-testnet-bridge.git
cd miden-testnet-bridge
cp .env.example .env
```

Set a unique Miden master seed before starting the bridge. Do not reuse the
placeholder across public testnet runs; reusing the same seed reuses the same
solver account and can cause account commitment conflicts after that account has
advanced on-chain.

```bash
perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
```

The checked-in `.env.example` is Compose-oriented. If you run host-side cargo
tests directly against Anvil, set `EVM_RPC_URL=http://localhost:8545` in that
shell instead of sourcing the Compose `.env`.

Start the default stack:

```bash
docker compose up -d --build
```

Wait for the bridge to become healthy. Bootstrap submits several Miden testnet
transactions, so the first healthy response can take a few minutes.

```bash
curl -i http://localhost:8080/healthz
curl -i http://localhost:8080/readyz
curl -s http://localhost:8080/v0/tokens
```

`/healthz` is local service liveness. `/readyz` also checks the configured
Miden RPC and can briefly return `503` during testnet RPC lag.

Tear down local state when you want a clean run:

```bash
docker compose down --volumes --remove-orphans
```

For the one-command sandbox path, prefer:

```bash
cp .env.anvil.example .env
make sandbox
```

`make sandbox` generates a fresh `MIDEN_MASTER_SEED_HEX` if `.env` still has the
placeholder, starts the Compose stack, waits for health, and prints the Bridge
API, `/lab`, and `bridgectl` entry points.

## Builder CLI

`bridgectl` is the local operator CLI for the mock API.

```bash
./bin/bridgectl status
./bin/bridgectl tokens
./bin/bridgectl quote inbound --asset eth --amount 1000000000000
./bin/bridgectl quote outbound --asset eth --amount 1000000000000 --recipient 0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc --refund-to <miden-address>
./bin/bridgectl demo inbound
./bin/bridgectl demo claim <miden-account-id>
./bin/bridgectl demo outbound-fund
./bin/bridgectl demo outbound-submit <miden-account-id>
./bin/bridgectl flows
./bin/bridgectl flow <correlation-id>
./bin/bridgectl logs
./bin/bridgectl reset
```

Set `BRIDGE_URL=http://host:port` to point the CLI at a non-default bridge.
`bridgectl quote` infers the EVM asset prefix from `/demo/info`, so it uses
`eth-anvil:*` in Anvil mode and `eth-sepolia:*` in Sepolia mode.

## Sepolia-Ready Profile

Sepolia mode keeps the same NEAR Intents 1Click-shaped surface, but it does not
scan Sepolia from genesis. The builder submits the real EVM tx hash after the
deposit lands, and the bridge verifies that transaction before settlement:

```bash
cp .env.sepolia.example .env
# Fill EVM_RPC_URL, MASTER_MNEMONIC, SOLVER_PRIVATE_KEY, and MIDEN_MASTER_SEED_HEX.
make sepolia
./bin/bridgectl tokens
./bin/bridgectl quote inbound --asset eth --amount 1000000000000 --recipient <miden-address> --refund-to <sepolia-address>
```

After sending Sepolia ETH to the returned `depositAddress`, notify the mock:

```bash
curl -s http://localhost:8080/v0/deposit/submit \
  -H 'content-type: application/json' \
  -d '{"txHash":"0x...","depositAddress":"0x..."}'
```

Native ETH is enabled by default as `eth-sepolia:eth`. ERC20 Sepolia assets are
advertised only when `EVM_TOKEN_ADDRESSES_PATH` points at a JSON file with token
addresses for `usdc`, `usdt`, or `btc`.

Run the live Sepolia evidence path after the solver and test user keys are
funded:

```bash
RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
```

The runner drives both directions through the mock 1Click `/v0/*` API:

- Sepolia ETH deposit -> `/v0/deposit/submit` -> Miden public P2ID mint -> user
  claim.
- Sepolia ETH deposit to fund the user's Miden source account -> Miden public
  `BridgeOutV1` note from that user account -> bridge consume -> Sepolia ETH
  release.

It reads `.env`, never prints private keys, and defaults to
`LIVE_E2E_DATABASE_URL=postgres://postgres:postgres@localhost:5432/miden_bridge`
for lifecycle evidence from the Compose Postgres port.

Latest live evidence:

```text
SEPOLIA_E2E_EVIDENCE inbound correlation_id=3e5ec16b-2fa2-4c0a-8ce7-0ae8725bbac1 evm_deposit_tx_hash=0xaca72ebac72cfbda3cd5957605b8e01f107c37acc9d2bfe118552b0c7cab311a miden_mint_tx_ids=["0x7a4c0ed23a0b13eb4f559ed2f9f82282b38b99dda2138a9b1e94759b3aefa0b6"] claim_tx_id=0x6bf05ee2a2d9f772823abe66cf2417995ecaa71fdbe731b247694ec0f66eccfb
SEPOLIA_E2E_EVIDENCE outbound funding_correlation_id=8b1920ef-ca3b-4fbc-82c5-35f6f8670332 outbound_correlation_id=ab2b3f5d-0d5e-4b86-a013-0f4ddcef05aa funding_evm_deposit_tx_hash=0x3c0e444fa726496ee09cda9c72d2d14d8a07235de81bdf8de94d0a559c899644 bridge_out_note_tx_id=0xe9db64f8a00db3527ebb1f5d443c09ce2ec80c639ccabd7e5b4b6195ea045f2d miden_consume_tx_ids=["0xbaa1789bb950b97bb8300aaebc53e817760f4791ce04b5d971b85f69e4577f81"] evm_release_tx_hashes=["0x23640d4ad68277a065fa6ec70cc26b6bc7d2acf181bf0b6669da1b03fa668885"] balance_delta_wei=1000000000000
```

## Clickable Lab

The lab UI is served by the bridge container:

```text
http://localhost:8080/lab
```

It visualizes both directions:

- **Anvil -> Miden:** quote, Anvil deposit, Bridge API processing,
  solver-signed public P2ID note on Miden, recipient claim.
- **Miden -> Anvil:** funding quote, Miden wallet funding, public
  `BridgeOutV1` note, bridge consume, Anvil release.

The lab uses `/demo/*` endpoints for operations a browser cannot safely perform
itself, while preserving the `/v0/*` API as the NEAR Intents-compatible mock
surface. It also embeds Mermaid diagrams for the solution shape and both
transfer directions so a builder can understand the flow before clicking it.

## Reproduce The E2E Evidence

Run the non-E2E regression set first:

```bash
cargo fmt --check
cargo test --lib --test evm --test hardening --test lifecycle --test miden_bridge --test miden_node --test state
```

Run the full public-testnet E2E suite:

```bash
RUSTFLAGS='-C debug-assertions=no' RUN_E2E=1 cargo test --test e2e -- --nocapture --test-threads=1 2>&1 | tee e2e.log
```

Expected shape:

```text
test result: ok. 5 passed; 0 failed
```

Extract the evidence lines:

```bash
grep -E 'E2E_EVIDENCE|test result:' e2e.log
```

The recorded evidence run for this pivot produced:

```text
test result: ok. 5 passed; 0 failed; finished in 934.77s
```

The current static evidence report is checked in at
`docs/smoke-test-report.html` and published here:

```text
https://brianseong99.github.io/miden-testnet-bridge/smoke-test-report.html
```

## Flow Details

### Inbound: EVM To Miden

```mermaid
sequenceDiagram
    participant User
    participant Anvil
    participant BridgeAPI as Bridge API
    participant MidenTestnet
    participant Recipient

    User->>BridgeAPI: POST /v0/quote eth-anvil:eth -> miden-testnet:eth
    BridgeAPI-->>User: EVM depositAddress
    User->>Anvil: send ETH to depositAddress
    BridgeAPI->>Anvil: detect and confirm deposit
    BridgeAPI->>BridgeAPI: KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT -> PROCESSING
    BridgeAPI->>MidenTestnet: submit solver-signed public P2ID mint to recipient
    MidenTestnet-->>BridgeAPI: public P2ID note committed and consumable
    BridgeAPI->>BridgeAPI: append miden_mint_tx_id and mark quote SUCCESS
    Recipient->>MidenTestnet: sync public note targeted to recipient account
    Recipient->>MidenTestnet: consume and claim P2ID note
    MidenTestnet-->>Recipient: recipient balance updated
```

### Outbound: Miden To EVM

```mermaid
sequenceDiagram
    participant UserClient
    participant BridgeAPI as Bridge API
    participant MidenTestnet
    participant BridgePoller
    participant Anvil

    UserClient->>BridgeAPI: POST /v0/quote miden-testnet:eth -> eth-anvil:eth
    BridgeAPI-->>UserClient: stable bridge account plus BridgeOutV1 depositMemo
    UserClient->>MidenTestnet: create public BridgeOutV1 note with assets
    BridgePoller->>MidenTestnet: sync and scan public notes
    BridgePoller->>BridgePoller: validate target account, quote hash, faucet, amount
    BridgePoller->>MidenTestnet: consume matched note with bridge account
    MidenTestnet-->>BridgePoller: consume tx committed
    BridgePoller->>Anvil: release funds to EVM recipient
    BridgePoller->>BridgeAPI: persist tx ids and mark SUCCESS
```

### Restart Recovery

```mermaid
flowchart TD
    A["Quote is PROCESSING"] --> B["Bridge container restarts"]
    B --> C["Reload non-terminal quote from Postgres"]
    C --> D{"Durable Miden tx id exists?"}
    D -->|Yes| E["Wait for existing tx confirmation"]
    D -->|No| F{"Only idempotency key exists?"}
    F -->|Yes| G["Treat as interrupted before durable submit"]
    F -->|No| H["Submit Miden tx"]
    G --> H
    H --> I["Persist Miden tx id"]
    E --> J["Mark SUCCESS after confirmation"]
    I --> J
```

## Environment

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | `postgres://postgres:postgres@postgres:5432/miden_bridge` | Postgres DSN used by the bridge service. |
| `MIDEN_RPC_URL` | Yes | `https://rpc.testnet.miden.io` | Public Miden testnet RPC endpoint. Set to `http://miden-node:57291` only for legacy local-node mode. |
| `MIDEN_REMOTE_PROVER_URL` | No | Native `miden-client` testnet/devnet default | Optional remote transaction prover override. Public testnet works without setting this. |
| `MIDEN_REMOTE_PROVER_TIMEOUT_SECS` | No | `180` | Timeout for remote transaction prover requests. Public testnet proving can exceed the SDK's default 10 seconds during bootstrap. |
| `MIDEN_MASTER_SEED_HEX` | Yes for reproducible public testnet runs | Compose fallback is a fixed test seed | 32-byte hex seed used to derive the deterministic Miden solver and faucet accounts. Use a fresh value for each public testnet run. |
| `MIDEN_STORE_DIR` | Yes | `/var/lib/bridge/miden-store` in Compose, `./.miden-store` for host runs | Persistent SQLite store plus keystore for the Rust Miden client. |
| `EVM_RPC_URL` | Yes | `http://anvil:8545` in Compose | EVM RPC endpoint. The validated paths use local Anvil and Sepolia through `https://gateway.tenderly.co/public/sepolia`. |
| `MASTER_MNEMONIC` | Yes | Anvil default mnemonic in Compose | Seed material for deterministic EVM quote wallet derivation. |
| `SOLVER_PRIVATE_KEY` | Yes | Anvil default account key in Compose | Solver-side EVM key used for local Anvil release and refund transactions. |
| `EVM_CHAIN_ID` | No | `271828` | Local Anvil chain id. |
| `EVM_TOKEN_ADDRESSES_PATH` | No | `/state/token-addresses.json` | Address file produced by `anvil-init`. |
| `EVM_REQUIRED_CONFIRMATIONS` | No | `1` for Anvil, `2` in `.env.sepolia.example` | Number of EVM confirmations required before deposit confirmation or solver release/refund completion. |
| `EVM_DEPOSIT_SCAN_LOOKBACK_BLOCKS` | No | Empty | Optional bounded scan window. Leave empty for Sepolia so deposits are confirmed through `/v0/deposit/submit` tx hashes instead of RPC-heavy history scans. |
| `BRIDGE_HTTP_PORT` | No | `8080` | Host port exposed by the bridge service. |
| `BRIDGE_PROFILE` | No | `anvil` | Runtime profile. `anvil` is the default builder sandbox; `sepolia` enables live Sepolia native ETH with tx-hash based deposit confirmation. |
| `BRIDGE_DEMO_ENABLED` | No | `0` | Enables `/demo/*` orchestration endpoints. Set to `1` for the clickable sandbox. |
| `BRIDGE_UI_ENABLED` | No | `1` | Documents whether `/lab` should be treated as enabled by clients. |
| `BRIDGE_CORS_ALLOW_ORIGIN` | No | `*` | CORS allow-origin for third-party app builders testing from a browser. |
| `DEMO_EVM_FUNDED_PRIVATE_KEY` | No | Anvil default funded key | Key used by `/demo/*` to send Anvil deposits during clickable flows. |
| `BRIDGE_PRICER` | No | CoinGecko default when unset | E2E harness sets `mock` for deterministic quotes. |
| `RUST_LOG` | No | `info,sqlx=warn,hyper=warn,tower_http=warn` in Compose | Tracing filter. |
| `LOG_FORMAT` | No | `json` | `json` or `pretty`. |

## Local CI

Run the Dockerized local gate before opening or updating a PR:

```bash
bash scripts/ci.sh
```

Run the E2E suite separately because it talks to public Miden testnet:

```bash
make e2e
```

The E2E tests are serialized by design. Each test creates fresh Miden testnet
accounts and uses the native `miden-client` remote prover path.

## Publishing The Evidence Page

Maintainers can publish `docs/smoke-test-report.html` through GitHub Pages from
the `gh-pages` branch:

```bash
pages_dir=/tmp/miden-testnet-bridge-gh-pages
rm -rf "$pages_dir"
git fetch origin gh-pages
git worktree add -B gh-pages "$pages_dir" origin/gh-pages
mkdir -p "$pages_dir/docs"
cp docs/smoke-test-report.html "$pages_dir/docs/smoke-test-report.html"
cd "$pages_dir"
git add docs/smoke-test-report.html
git commit -m "docs: update smoke test report [skip ci]"
git push origin gh-pages
cd -
git worktree remove "$pages_dir"
git branch -D gh-pages
```

Triggering a Pages rebuild is optional, but useful when checking propagation:

```bash
gh api -X POST repos/:owner/:repo/pages/builds --jq '{status:.status,url:.url}'
```

## Local-Node Mode

Local-node mode is legacy/manual only. It is useful for isolated experiments, not
for the accepted bridge evidence.

Seed the local Miden node:

```bash
make genesis
```

Point the bridge to the local node and start the profile-gated services:

```bash
MIDEN_RPC_URL=http://miden-node:57291 docker compose --profile local-node up -d
```

Run the local-node E2E fallback:

```bash
make e2e-local-node
```

## Troubleshooting

- `incorrect account initial commitment`: use a fresh `MIDEN_MASTER_SEED_HEX` and
  a clean `MIDEN_STORE_DIR` or clean Compose volumes.
- Slow `bridge` healthcheck: public Miden testnet bootstrap submits several
  transactions. Give it the full startup window before assuming failure.
- Missing E2E tests: set `RUN_E2E=1`. The suite intentionally skips without it.
- E2E debug assertion failures: keep `RUSTFLAGS='-C debug-assertions=no'` visible
  until the upstream Miden debug-assertion issue is removed from the path.
- Sepolia claims: report Sepolia as validated only when a run includes live
  Sepolia tx hashes, funded solver balances, and final status evidence. The
  2026-05-15 native ETH run satisfies that bar.

## More Detail

See `docs/E2E_HANDOFF.md` for the implementation handoff, evidence ids,
residual risks, and Sepolia validation notes.
