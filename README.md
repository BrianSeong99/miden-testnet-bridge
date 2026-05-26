# miden-testnet-bridge

Mock **NEAR Intents 1Click** bridge between Sepolia and Miden testnet.

This repo is a builder sandbox: third-party app teams can run a local mock of
the NEAR Intents 1Click API, point their app at it, and test the same
quote/deposit/status flow they would use against a hosted 1Click service. The
default documented profile is public Miden testnet plus Sepolia native ETH.

This is testnet-only infrastructure for local testing against Sepolia and public
Miden testnet. It is not a production bridge, not a mainnet integration path,
and must not be used with mainnet funds.

The primary path in this repo is Sepolia native ETH plus public Miden testnet.

## Terminal Walkthrough

The video below walks through the Sepolia path step by step: quote request,
native ETH deposit, tx-hash submit, bridge verification, Miden claim, and the
outbound public `BridgeOutV1` note flow.

[![Terminal walkthrough preview](docs/assets/miden-testnet-bridge-terminal-demo-poster.jpg)](docs/assets/miden-testnet-bridge-terminal-demo.mp4)

Direct MP4:
[`docs/assets/miden-testnet-bridge-terminal-demo.mp4`](docs/assets/miden-testnet-bridge-terminal-demo.mp4)

The renderer lives in [`tools/walkthrough-video`](tools/walkthrough-video) and
is opt-in. Normal bridge setup does not install the video dependencies.

## Builder Quick Start

For the full step-by-step walkthrough, start with
[`docs/builder-testing-guide.md`](docs/builder-testing-guide.md).

```bash
git clone <repo-url>
cd miden-testnet-bridge
cp .env.sepolia.example .env
```

Fill `.env` with testnet-only values:

```text
EVM_RPC_URL=https://gateway.tenderly.co/public/sepolia
MASTER_MNEMONIC=<builder-controlled-test-mnemonic>
SOLVER_PRIVATE_KEY=<funded-sepolia-solver-private-key>
DEMO_EVM_FUNDED_PRIVATE_KEY=<funded-sepolia-test-user-private-key>
MIDEN_MASTER_SEED_HEX=<fresh-32-byte-hex-seed>
```

Generate a fresh Miden seed:

```bash
perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
```

Start the Sepolia profile:

```bash
make sepolia
```

Monitor from the CLI:

```bash
./bin/bridgectl status
./bin/bridgectl tokens
./bin/bridgectl flows
./bin/bridgectl flow <correlation-id>
make sepolia-logs
make sepolia-reset
```

Run live Sepolia evidence after the solver and test-user keys are funded:

```bash
RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
```

## API Surface

The primary integration surface remains NEAR Intents 1Click-shaped:

```text
GET  /v0/tokens
POST /v0/quote
POST /v0/deposit/submit
GET  /v0/status
```

`/demo/*` and the clickable lab UI are Sepolia testnet helpers. They exist for
walkthroughs and manual demos with funded test keys. App integrations should
still use `/v0/*`.

## AggLayer Testnet Helper

The lab also exposes an AggLayer mode for the public Bali/Sepolia testnet path
from `0xMiden/miden-client#2173`. The backend is a dry-run command planner,
not a custody service. The frontend can hand the planned Sepolia transaction to
an injected browser wallet only after the user reviews and confirms it.

```text
GET  /agglayer/info
POST /agglayer/l1/deposit/plan
POST /agglayer/l2/withdraw/plan
POST /agglayer/l2/withdraw/claim/plan
```

The helper records the post-relaunch constants from the PR review notes:

```text
DEST_NETWORK=76
L2_CHAIN_ID=1022211914
MIDEN_BRIDGE_ID=mcst1arychvrurzxdy5qwz0mg5p5umsvsepyx
MIDEN_FAUCET_ID=mcst1arnrhfau9svl7cpu2tr8lfzzd5j87wwe
```

Sepolia to Miden returns the padded bridge destination, `bridgeAsset` calldata,
a Gateway FM status URL, and a dry-run `cast send` command. Miden to Sepolia
returns the `bridge-out-tool` command shape, the correct `/bridges/{address}`
readiness URL, a `/claims/{address}` history URL, and a `claimAsset` command
template. After the B2AGG note is submitted, poll `/bridges/{address}` for a
Miden-origin row with `ready_for_claim=true`, `dest_net=0`, and an empty
`claim_tx_hash`. Do not use `/claims/{address}` as the readiness check; it is
empty until a manual `claimAsset` transaction lands.

When a row is ready, `POST /agglayer/l2/withdraw/claim/plan` with the Sepolia
recipient address. The helper fetches the Gateway FM merkle proof and returns a
dry-run `cast send ... claimAsset(...)` command. Broadcasting remains an
explicit operator action with a funded Sepolia test key.

The lab UI includes a native injected-wallet connector for Sepolia wallets
such as MetaMask or Rabby. For the NEAR Intents mock flow, Sepolia deposits are
sent from the connected browser wallet and submitted back to `/v0/deposit/submit`.
For Miden, the UI accepts a testnet account ID because there is not yet a
standard injected Miden browser wallet provider in this mock service.

## What This Proves

- Inbound: a Sepolia native ETH deposit is verified through
  `/v0/deposit/submit`, then the Bridge API submits a solver-signed public P2ID
  note on Miden testnet for the recipient.
- Outbound: a user creates a public programmable `BridgeOutV1` note on Miden
  testnet, the bridge consumes that note, then releases Sepolia ETH.
- Restart recovery: quote state, tx ids, and lifecycle transitions are durable
  in Postgres.
- Evidence logging: Sepolia runs print correlation ids, Miden tx ids, Sepolia
  tx hashes, and final lifecycle evidence.

The 2026-05-15 live run used public Miden testnet plus Sepolia through
`https://gateway.tenderly.co/public/sepolia`, with public Sepolia tx hashes and
Miden tx ids recorded in `docs/smoke-test-report.html`.

Evidence artifact:

```text
docs/smoke-test-report.html
```

## Bridge Shape

```mermaid
flowchart LR
    User["Builder app / user"] --> BridgeAPI["Mock 1Click Bridge API"]
    BridgeAPI --> Postgres["Postgres quote state"]
    BridgeAPI --> Sepolia["Sepolia native ETH"]
    BridgeAPI --> Miden["Miden testnet via miden-client"]
    Miden --> Notes["Public Miden notes"]
    Notes --> Claim["Recipient or bridge consumes note"]
    Claim --> Final["Release, refund, or wallet balance update"]
```

The mock follows the NEAR Intents 1Click lifecycle:

1. Builder app fetches supported assets from `/v0/tokens`.
2. Builder app requests a quote from `/v0/quote`.
3. User sends the origin-chain deposit to the returned deposit address or
   creates the returned Miden public-note deposit.
4. For Sepolia deposits, the builder submits the landed tx hash with
   `/v0/deposit/submit`.
5. Builder app polls `/v0/status` until `SUCCESS`, `REFUNDED`, or `FAILED`.

Inbound completion has two layers:

1. The bridge marks the quote `SUCCESS` after the solver-signed public P2ID note
   is committed and consumable on Miden.
2. The recipient completes wallet-side settlement by syncing and consuming that
   public P2ID note.

Outbound uses public programmable notes because Miden accounts are not reliably
discoverable before they have sent a transaction. The bridge watches for a
public `BridgeOutV1` note targeted to the stable bridge account instead of
deriving one deposit account per quote.

## Prerequisites

- Docker with Compose v2.
- Rust toolchain compatible with edition 2024. The Docker CI path uses
  `rust:1.93-slim`.
- OpenSSL for fresh Miden seeds.
- `curl` and optionally `jq`.
- Network access to `https://rpc.testnet.miden.io`.
- A Sepolia RPC endpoint.
- Sepolia ETH on the test-only solver key and test-user key. For the mock
  NEAR Intents path, `SOLVER_PRIVATE_KEY` must hold enough Sepolia ETH for both
  destination releases and release/refund gas; otherwise Miden -> Sepolia quotes
  can stay in `PROCESSING` after the Miden note is consumed.

No local Miden node is required for the supported path. The bridge uses
`miden-client` network defaults for Miden testnet, including the native remote
transaction prover configuration.

## Sepolia `/v0/*` Examples

### Inbound: Sepolia To Miden

```bash
curl -s http://localhost:8080/v0/quote \
  -H 'content-type: application/json' \
  -d '{
    "dry": false,
    "depositMode": "SIMPLE",
    "swapType": "EXACT_INPUT",
    "slippageTolerance": 100.0,
    "originAsset": "eth-sepolia:eth",
    "depositType": "ORIGIN_CHAIN",
    "destinationAsset": "miden-testnet:eth",
    "amount": "1000000000000",
    "refundTo": "<sepolia-refund-address>",
    "refundType": "ORIGIN_CHAIN",
    "recipient": "<miden-recipient-address>",
    "recipientType": "DESTINATION_CHAIN",
    "deadline": "2027-01-01T00:00:00Z"
  }' | jq .
```

After sending Sepolia ETH to `quote.depositAddress`, notify the mock:

```bash
curl -s http://localhost:8080/v0/deposit/submit \
  -H 'content-type: application/json' \
  -d '{"txHash":"0x...","depositAddress":"<deposit-address>"}' | jq .
```

Poll:

```bash
curl -s "http://localhost:8080/v0/status?depositAddress=<deposit-address>" | jq .
```

### Outbound: Miden To Sepolia

```bash
curl -s http://localhost:8080/v0/quote \
  -H 'content-type: application/json' \
  -d '{
    "dry": false,
    "depositMode": "SIMPLE",
    "swapType": "EXACT_INPUT",
    "slippageTolerance": 100.0,
    "originAsset": "miden-testnet:eth",
    "depositType": "ORIGIN_CHAIN",
    "destinationAsset": "eth-sepolia:eth",
    "amount": "1000000000000",
    "refundTo": "<miden-refund-address>",
    "refundType": "ORIGIN_CHAIN",
    "recipient": "<sepolia-recipient-address>",
    "recipientType": "DESTINATION_CHAIN",
    "deadline": "2027-01-01T00:00:00Z"
  }' | jq .
```

The response returns:

```text
correlationId
quote.depositAddress
quote.depositMemo
```

`quote.depositAddress` is the stable Miden bridge account.
`quote.depositMemo` is the `BridgeOutV1` instruction payload. The user creates
a public Miden note carrying the quoted asset and memo, then the bridge poller
consumes that note and releases Sepolia ETH.

The mock service is solver-funded on the Sepolia side: the configured
`SOLVER_PRIVATE_KEY` sends the destination release transaction. Keep that key
pre-funded with the release amount plus Sepolia gas before running this flow.

Poll:

```bash
curl -G http://localhost:8080/v0/status \
  --data-urlencode "depositAddress=<miden-bridge-account-address>" \
  --data-urlencode "depositMemo=<deposit-memo>" | jq .
```

## Live Sepolia Evidence

Run:

```bash
RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
```

The runner drives both directions through the mock 1Click `/v0/*` API:

- Sepolia ETH deposit -> `/v0/deposit/submit` -> Miden public P2ID mint -> user
  claim.
- Sepolia ETH deposit to fund the user's Miden source account -> Miden public
  `BridgeOutV1` note from that user account -> bridge consume -> Sepolia ETH
  release from the funded solver key.

It reads `.env`, never prints private keys, and defaults to
`LIVE_E2E_DATABASE_URL=postgres://postgres:postgres@localhost:5432/miden_bridge`
for lifecycle evidence from the Compose Postgres port.

Latest live evidence:

```text
SEPOLIA_E2E_EVIDENCE inbound correlation_id=3e5ec16b-2fa2-4c0a-8ce7-0ae8725bbac1 evm_deposit_tx_hash=0xaca72ebac72cfbda3cd5957605b8e01f107c37acc9d2bfe118552b0c7cab311a miden_mint_tx_ids=["0x7a4c0ed23a0b13eb4f559ed2f9f82282b38b99dda2138a9b1e94759b3aefa0b6"] claim_tx_id=0x6bf05ee2a2d9f772823abe66cf2417995ecaa71fdbe731b247694ec0f66eccfb
SEPOLIA_E2E_EVIDENCE outbound funding_correlation_id=8b1920ef-ca3b-4fbc-82c5-35f6f8670332 outbound_correlation_id=ab2b3f5d-0d5e-4b86-a013-0f4ddcef05aa funding_evm_deposit_tx_hash=0x3c0e444fa726496ee09cda9c72d2d14d8a07235de81bdf8de94d0a559c899644 bridge_out_note_tx_id=0xe9db64f8a00db3527ebb1f5d443c09ce2ec80c639ccabd7e5b4b6195ea045f2d miden_consume_tx_ids=["0xbaa1789bb950b97bb8300aaebc53e817760f4791ce04b5d971b85f69e4577f81"] evm_release_tx_hashes=["0x23640d4ad68277a065fa6ec70cc26b6bc7d2acf181bf0b6669da1b03fa668885"] balance_delta_wei=1000000000000
```

## Flow Details

### Inbound: Sepolia To Miden

```mermaid
sequenceDiagram
    participant User
    participant Sepolia
    participant BridgeAPI as Bridge API
    participant MidenTestnet
    participant Recipient

    User->>BridgeAPI: POST /v0/quote eth-sepolia:eth to miden-testnet:eth
    BridgeAPI-->>User: Sepolia depositAddress
    User->>Sepolia: send native ETH to depositAddress
    User->>BridgeAPI: POST /v0/deposit/submit with tx hash
    BridgeAPI->>Sepolia: verify tx pays quoted deposit address
    BridgeAPI->>BridgeAPI: KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT -> PROCESSING
    BridgeAPI->>MidenTestnet: submit solver-signed public P2ID mint
    MidenTestnet-->>BridgeAPI: public P2ID note committed and consumable
    BridgeAPI->>BridgeAPI: append miden_mint_tx_id and mark quote SUCCESS
    Recipient->>MidenTestnet: consume P2ID note
    MidenTestnet-->>Recipient: recipient balance updated
```

### Outbound: Miden To Sepolia

```mermaid
sequenceDiagram
    participant UserClient
    participant BridgeAPI as Bridge API
    participant MidenTestnet
    participant BridgePoller
    participant Sepolia

    UserClient->>BridgeAPI: POST /v0/quote miden-testnet:eth to eth-sepolia:eth
    BridgeAPI-->>UserClient: stable bridge account plus BridgeOutV1 depositMemo
    UserClient->>MidenTestnet: create public BridgeOutV1 note with assets
    BridgePoller->>MidenTestnet: sync and scan public notes
    BridgePoller->>BridgePoller: validate target account, quote hash, faucet, amount
    BridgePoller->>MidenTestnet: consume matched note with bridge account
    MidenTestnet-->>BridgePoller: consume tx committed
    BridgePoller->>Sepolia: release native ETH to EVM recipient
    BridgePoller->>BridgeAPI: persist tx ids and mark SUCCESS
```

## Environment

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | `postgres://postgres:postgres@postgres:5432/miden_bridge` | Postgres DSN used by the bridge service. |
| `MIDEN_RPC_URL` | Yes | `https://rpc.testnet.miden.io` | Public Miden testnet RPC endpoint. Local-node mode is legacy only. |
| `MIDEN_REMOTE_PROVER_URL` | No | Native `miden-client` testnet/devnet default | Optional remote transaction prover override. Public testnet works without setting this. |
| `MIDEN_REMOTE_PROVER_TIMEOUT_SECS` | No | `180` | Timeout for remote transaction prover requests. |
| `MIDEN_MASTER_SEED_HEX` | Yes | none | 32-byte hex seed used to derive deterministic Miden solver and faucet accounts. Use a fresh value for each public testnet run. |
| `MIDEN_STORE_DIR` | Yes | `/var/lib/bridge/miden-store` in Compose | Persistent SQLite store plus keystore for the Rust Miden client. |
| `EVM_RPC_URL` | Yes | `https://gateway.tenderly.co/public/sepolia` in `.env.sepolia.example` | Sepolia RPC endpoint. |
| `MASTER_MNEMONIC` | Yes | none | Seed material for deterministic Sepolia quote wallet derivation. Use a test mnemonic only. |
| `SOLVER_PRIVATE_KEY` | Yes | none | Funded Sepolia test key used for release and refund transactions. For Miden -> Sepolia mock releases it must cover the destination amount plus gas. |
| `EVM_CHAIN_ID` | No | `11155111` | Sepolia chain id. |
| `EVM_TOKEN_ADDRESSES_PATH` | No | `/state/token-addresses.json` | Optional Sepolia ERC20 token-address file. Native ETH works without ERC20 addresses. |
| `EVM_REQUIRED_CONFIRMATIONS` | No | `2` | Number of Sepolia confirmations required before deposit confirmation or solver release/refund completion. |
| `EVM_DEPOSIT_SCAN_LOOKBACK_BLOCKS` | No | Empty | Leave empty for Sepolia so deposits are confirmed through `/v0/deposit/submit`. |
| `BRIDGE_HTTP_PORT` | No | `8080` | Host port exposed by the bridge service. |
| `BRIDGE_PROFILE` | No | `sepolia` in `compose.sepolia.yaml` | Runtime profile. |
| `BRIDGE_DEMO_ENABLED` | No | `0` | Demo endpoints are disabled in the Sepolia profile. |
| `BRIDGE_UI_ENABLED` | No | `1` | Documents whether `/lab` should be treated as enabled by clients. |
| `BRIDGE_CORS_ALLOW_ORIGIN` | No | `*` | CORS allow-origin for third-party app builders testing from a browser. |
| `DEMO_EVM_FUNDED_PRIVATE_KEY` | Yes for `sepolia_e2e` | none | Funded Sepolia test-user key used by the live evidence runner. |
| `BRIDGE_PRICER` | No | CoinGecko default when unset | E2E harness sets `mock` for deterministic quotes. |
| `RUST_LOG` | No | `info,sqlx=warn,hyper=warn,tower_http=warn` in Compose | Tracing filter. |
| `LOG_FORMAT` | No | `json` | `json` or `pretty`. |

## Local CI

Run the Dockerized local gate before opening or updating a PR:

```bash
bash scripts/ci.sh
```

Run non-E2E tests:

```bash
cargo fmt --check
cargo test --lib --test evm --test hardening --test lifecycle --test miden_bridge --test miden_node --test state
```

The Rust `e2e` test target is still useful for regression coverage. Treat live
Sepolia evidence from `sepolia_e2e` as the public testnet validation path.

## Local Sepolia Lab

For the clickable lab:

```bash
cp .env.sepolia.example .env
make sandbox
```

Open:

```text
http://localhost:3000
```

The bridge still serves the legacy static helper at `http://localhost:8080/lab`.

## Local-Node Mode

Local-node mode is legacy/manual only. It is useful for isolated experiments,
not for accepted bridge evidence.

```bash
make genesis
MIDEN_RPC_URL=http://miden-node:57291 docker compose --profile local-node up -d
make e2e-local-node
```

## Troubleshooting

- `make sepolia` refuses to start: placeholders remain in `.env`. Fill Sepolia
  RPC, mnemonic, funded solver key, funded test-user key, and fresh Miden seed.
- `incorrect account initial commitment`: use a fresh `MIDEN_MASTER_SEED_HEX`
  and clean Compose volumes.
- Slow `bridge` healthcheck: public Miden testnet bootstrap submits several
  transactions. Give it the full startup window before assuming failure.
- Sepolia deposits do not progress: submit the real deposit tx hash through
  `/v0/deposit/submit`; Sepolia mode intentionally does not scan from genesis.
- E2E debug assertion failures: keep `RUSTFLAGS='-C debug-assertions=no'`
  visible until the upstream Miden debug-assertion issue is removed from the
  path.

## More Detail

- [`docs/builder-testing-guide.md`](docs/builder-testing-guide.md): Sepolia
  builder tutorial.
- [`docs/RUNBOOK.md`](docs/RUNBOOK.md): operator recovery procedures.
- [`docs/smoke-test-report.html`](docs/smoke-test-report.html): recorded
  Sepolia evidence page.
- [`AGENTS.md`](AGENTS.md): canonical repo operating instructions for agents.
- [`AGENT.md`](AGENT.md): compatibility startup guide for agents.
