# Collaboration Guide

## Milestone

- Milestone: repository milestone 1

## Public Text Rules

- Do not add attribution trailers, origin notes, tool branding, or generated-by
  text in commits, docs, comments, pull requests, or GitHub issue updates.
- Write public artifacts under the repository maintainer's identity only.

## Supported Reproduction Path

- Treat this repository as a mock NEAR Intents 1Click builder sandbox. The
  public integration surface should stay aligned with the 1Click flow:
  `/v0/tokens`, `/v0/quote`, optional `/v0/deposit/submit`, `/v0/status`.
- Treat this as testnet-only infrastructure for local testing, Sepolia, and
  public Miden testnet. Do not present it as a production bridge, a mainnet
  integration path, or something that should ever handle mainnet funds.
- `/demo/*` and `/lab` are local sandbox helpers. Do not make third-party app
  integrations depend on demo-only endpoints.
- Default all builder-facing docs and guides to public Miden testnet at
  `https://rpc.testnet.miden.io` plus Sepolia native ETH.
- `/demo/*` and `/lab` are Sepolia helpers. Do not reintroduce a local EVM default
  path unless a task explicitly scopes it as legacy regression-only work.
- Do not use the local Miden node for acceptance evidence. Local-node mode is a
  legacy/manual fallback only.
- Use native `miden-client` network behavior for testnet and devnet. Do not
  hand-assemble local-node defaults when the RPC URL is a known public network.
- Public testnet uses the native `miden-client` remote prover endpoint with the
  configured `MIDEN_REMOTE_PROVER_TIMEOUT_SECS`. Override the endpoint only when
  a task explicitly provides `MIDEN_REMOTE_PROVER_URL`.
- Keep `RUSTFLAGS='-C debug-assertions=no'` on E2E commands until the Miden
  debug-assertion path is no longer present.
- Sepolia profile uses `eth-sepolia:*` asset ids. Do not scan Sepolia from
  genesis; require `/v0/deposit/submit` with a real tx hash unless a task
  explicitly sets a bounded `EVM_DEPOSIT_SCAN_LOOKBACK_BLOCKS`.

## Bridge Semantics

- Inbound means EVM deposit to Miden payout:
  - User requests an EVM-to-Miden quote from the Bridge API.
  - The user deposits to the returned EVM deposit address.
  - The Bridge API detects and confirms the deposit.
  - The Bridge API submits a solver-signed public P2ID mint to the recipient
    account on Miden.
  - Bridge `SUCCESS` means the public P2ID payout note is committed and
    consumable.
  - The recipient still needs to sync and consume the public P2ID note to update
    their wallet balance.
- Outbound means Miden public note to EVM release:
  - User requests a Miden-to-EVM quote.
  - The Bridge API returns a stable bridge account and `BridgeOutV1` memo.
  - The user creates a public programmable note with assets and the memo.
  - The bridge poller validates target account, quote hash, faucet, and amount.
  - The bridge poller consumes the note with the bridge account, then releases or
    refunds on the EVM side.
- Public notes are intentional for the bridge design being tested here. Do not
  redesign back to per-quote Miden deposit accounts unless the task explicitly
  changes the product requirement.

## Agent Runbook

1. Start by checking local state:

   ```bash
   git status --short --branch
   ```

2. Read the current reproduction docs before changing behavior:

   ```bash
   sed -n '1,340p' README.md
   sed -n '1,260p' docs/builder-testing-guide.md
   sed -n '1,280p' docs/E2E_HANDOFF.md
   ```

3. For the default Sepolia builder path, use:

   ```bash
   cp .env.sepolia.example .env
   perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
   # Fill EVM_RPC_URL, MASTER_MNEMONIC, funded SOLVER_PRIVATE_KEY,
   # and funded DEMO_EVM_FUNDED_PRIVATE_KEY with testnet-only values.
   make sepolia
   ./bin/bridgectl status
   ./bin/bridgectl tokens
   ```

4. For a clean manual reproduction, use a fresh public-testnet seed:

   ```bash
   test -f .env || cp .env.sepolia.example .env
   perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/" .env
   # Fill Sepolia test keys and RPC before starting.
   docker compose --env-file .env down --volumes --remove-orphans
   docker compose --env-file .env up -d --build --wait --wait-timeout 900
   docker compose exec bridge curl -fsS http://127.0.0.1:8080/healthz
   docker compose exec lab-ui node -e "fetch('http://127.0.0.1:3000/health').then(r => process.exit(r.ok ? 0 : 1))"
   ```

5. For regression evidence, run:

   ```bash
   cargo fmt --check
   cargo test --lib --test evm --test hardening --test lifecycle --test miden_bridge --test miden_node --test state
   ```

6. For live Sepolia evidence, run:

   ```bash
   RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
   rg 'SEPOLIA_E2E_EVIDENCE|evidence_report_path|final_status' sepolia-e2e-live.log
   ```

7. When updating evidence, capture:

   - command line used
   - final command result and evidence report path
   - each `SEPOLIA_E2E_EVIDENCE` line
   - Miden tx ids
   - EVM tx hashes
   - lifecycle status sequence
   - confirmation that the EVM side was Sepolia

8. Do not claim Sepolia validation unless the evidence includes live Sepolia tx
   hashes and final status responses for inbound and outbound flows.

## Review Flow

- First pass: implementation on the task branch.
- Second pass: independent review from another model before push when the change
  is behavioral or affects public evidence.
- Cross-check the issue acceptance list, local CI result, E2E result, and any
  manual smoke-test notes before merging.

## Working Rules

- Branch from `main` for every task branch.
- Keep the local Docker gate green with `bash scripts/ci.sh` before asking for
  review when code changes are involved.
- Default to minimal implementations that satisfy the active issue scope.
- Do not revert unrelated user changes in a dirty worktree.
- Prefer structured parsers/APIs over ad hoc string manipulation.
- Keep evidence pages and issue comments factual: what was run, where it ran,
  what passed, what remains unvalidated.

## Execution Mode

- Unsupervised mode is acceptable only for scoped repo tasks with a clear
  acceptance checklist.
- Stop and hand back if the task would require destructive git operations,
  external secrets, public funds, or scope changes not captured in the issue.
