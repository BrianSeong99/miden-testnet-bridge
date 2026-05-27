# Agent Startup Guide

This repository is a testnet-only mock NEAR Intents 1Click bridge for Sepolia
and public Miden testnet. It is not a production bridge, not a mainnet
integration path, and must not be used with mainnet funds.

The canonical operating instructions are in `AGENTS.md`. Read them before
changing behavior:

```bash
sed -n '1,220p' AGENTS.md
sed -n '1,260p' README.md
sed -n '1,320p' docs/builder-testing-guide.md
```

## Default Path

Use Sepolia native ETH plus public Miden testnet by default.

```bash
cp .env.sepolia.example .env
perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
```

Fill `.env` with testnet-only values:

```text
EVM_RPC_URL=https://gateway.tenderly.co/public/sepolia
MASTER_MNEMONIC=<builder-controlled-test-mnemonic>
SOLVER_PRIVATE_KEY=<funded-sepolia-solver-private-key>
DEMO_EVM_FUNDED_PRIVATE_KEY=<funded-sepolia-test-user-private-key>
```

Start and inspect:

```bash
make sepolia
./bin/bridgectl status
./bin/bridgectl tokens
```

Run live evidence only when the Sepolia keys are funded:

```bash
RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
rg 'SEPOLIA_E2E_EVIDENCE|evidence_report_path|final_status' sepolia-e2e-live.log
```

## Integration Surface

Third-party apps should use only:

```text
GET  /v0/tokens
POST /v0/quote
POST /v0/deposit/submit
GET  /v0/status
```

For wallet UI and wallet chat semantics, read
`docs/wallet-bridge-clarity.md`. User-facing language should start from
Cross-chain Receive, Cross-chain Send, and Claim in Bridge UI contexts. Swap
and Earn are wallet-level features, not Bridge UI modes. Then explain the
selected provider route.

For Miden wallet integration details, read
`frontend/docs/miden-frontend-integration.md`. The monorepo Next UI uses the
MidenFi wallet adapter for account connection and supports explicit
wallet-launch parameters. Real Miden balance, transaction, sync, and
claim/consume flows still require dedicated SDK-backed implementation evidence.

`/demo/*` and the clickable lab UI are Sepolia testnet helpers for manual
walkthroughs. Do not make app integrations depend on demo-only endpoints.

## Local Lab

Use the Dockerized Sepolia lab for manual demos:

```bash
cp .env.sepolia.example .env
perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
make sandbox
```

Open the HomeLab route or the configured lab UI URL.

## Evidence Rules

Do not claim Sepolia validation unless the evidence includes:

- live Sepolia tx hashes,
- Miden tx ids,
- final `SUCCESS` statuses for inbound and outbound,
- the command used to produce the evidence.

Run non-E2E regression checks before pushing code changes:

```bash
cargo fmt --check
cargo test --lib --test evm --test hardening --test lifecycle --test miden_bridge --test miden_node --test state
```

Public text must not include attribution trailers, origin notes, model branding,
or generated-by text.
