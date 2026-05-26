# RUNBOOK

> Testnet only: this runbook is for the Sepolia and public Miden testnet mock
> bridge profile. It is not a production runbook and must not be used for
> mainnet funds.

All commands assume you are at the repo root. The default operator path is:

```text
compose.yaml + Sepolia native ETH + public Miden testnet
```

## Start Sepolia Profile

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

For Miden -> Sepolia mock releases, the solver key is the Sepolia liquidity
source. It must hold the release amount plus gas before the bridge consumes the
Miden `BridgeOutV1` note.

Start:

```bash
make sepolia
```

Check:

```bash
curl -i http://localhost:8080/healthz
curl -i http://localhost:8080/readyz
./bin/bridgectl status
./bin/bridgectl tokens
```

## Sepolia Deposit Does Not Progress

Sepolia mode intentionally does not scan from genesis. A quote stays in
`PENDING_DEPOSIT` until the builder submits the real deposit tx hash:

```bash
curl -s http://localhost:8080/v0/deposit/submit \
  -H 'content-type: application/json' \
  -d '{"txHash":"0x...","depositAddress":"0x..."}' | jq .
```

Diagnostic commands:

```bash
./bin/bridgectl status
docker compose -f compose.sepolia.yaml --env-file .env logs bridge --tail=300 | rg "deposit|EVM|sepolia|reverted|does not pay"
docker compose -f compose.sepolia.yaml --env-file .env exec bridge printenv EVM_RPC_URL BRIDGE_PROFILE EVM_REQUIRED_CONFIRMATIONS EVM_DEPOSIT_SCAN_LOOKBACK_BLOCKS
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, deposit_address, status, evm_deposit_tx_hashes
  FROM quotes
  JOIN chain_artifacts USING (correlation_id)
  ORDER BY quotes.created_at DESC
  LIMIT 10;
"
```

If `/v0/deposit/submit` was called and the quote remains in
`KNOWN_DEPOSIT_TX`, verify the submitted Sepolia transaction:

- `to` must equal the quoted `depositAddress` for native ETH.
- `value` must be nonzero and at least the quoted amount.
- the receipt must be successful.
- the receipt block must have at least `EVM_REQUIRED_CONFIRMATIONS`.

## Miden RPC Or Prover Lag

Symptoms:

- `/readyz` returns non-200.
- Bridge logs show Miden sync, bootstrap, or proving failures.
- Inbound settlement cannot mint on Miden.
- Outbound note consumption stops progressing.

Diagnostic commands:

```bash
curl -i http://localhost:8080/healthz
curl -i http://localhost:8080/readyz
docker compose -f compose.sepolia.yaml --env-file .env ps
docker compose -f compose.sepolia.yaml --env-file .env logs bridge --tail=300 | rg "miden|bootstrap|sync failed|prover|rpc"
docker compose -f compose.sepolia.yaml --env-file .env exec bridge printenv MIDEN_RPC_URL MIDEN_REMOTE_PROVER_URL MIDEN_REMOTE_PROVER_TIMEOUT_SECS
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT *
  FROM miden_bootstrap;
"
```

Safe restart:

```bash
docker compose -f compose.sepolia.yaml --env-file .env restart bridge
curl -fsS http://localhost:8080/readyz
```

Do not switch to local Miden node mode for acceptance evidence.

## Stuck `PROCESSING` Quotes

Symptoms:

- Quote remains in `PROCESSING` for more than 30 minutes.
- Watchdog warnings appear in logs.
- No terminal `SUCCESS`, `FAILED`, or `REFUNDED` event is recorded.

Diagnostic commands:

```bash
docker compose -f compose.sepolia.yaml --env-file .env logs bridge --tail=300 | rg "processing quote exceeds watchdog threshold|SETTLEMENT_|SLIPPAGE_"
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, status, updated_at
  FROM quotes
  WHERE status = 'PROCESSING'
  ORDER BY updated_at ASC;
"
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, from_status, to_status, event_kind, reason, created_at
  FROM lifecycle_events
  WHERE correlation_id = '<correlation-id>'
  ORDER BY id;
"
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, evm_release_tx_hashes, miden_mint_tx_ids, evm_refund_tx_hashes, miden_refund_tx_ids
  FROM chain_artifacts
  WHERE correlation_id = '<correlation-id>';
"
```

Remediation:

1. Confirm whether settlement happened on-chain and only the database is stale,
   or whether no settlement transaction was submitted.
2. If a settlement tx exists in `miden_mint_tx_ids` or
   `evm_release_tx_hashes`, restart the bridge to let resume logic re-run
   idempotently:
   ```bash
   docker compose -f compose.sepolia.yaml --env-file .env restart bridge
   ```
3. If no settlement or refund tx exists, keep a human in the loop. There is no
   automatic fail path for stuck `PROCESSING` quotes by design.
4. Record the chosen action before editing state.
5. After remediation, re-run the SQL above and verify a new lifecycle event was
   appended.

## User Reports `REFUNDED` But No Refund Tx

An empty refund tx list is expected only when the last refund event reason is
`deadline_expired_no_deposit`. The quote expired before a confirmed deposit was
recorded, so the bridge moved to `REFUNDED` without broadcasting a refund.

Inspect:

```bash
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, status, deadline, updated_at
  FROM quotes
  WHERE correlation_id = '<correlation-id>';
"
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT id, from_status, to_status, event_kind, reason, metadata, created_at
  FROM lifecycle_events
  WHERE correlation_id = '<correlation-id>'
  ORDER BY id;
"
docker compose -f compose.sepolia.yaml --env-file .env exec postgres psql -U postgres -d miden_bridge -c "
  SELECT evm_deposit_tx_hashes, evm_refund_tx_hashes, miden_refund_tx_ids, idempotency_keys
  FROM chain_artifacts
  WHERE correlation_id = '<correlation-id>';
"
```

If a confirmed deposit exists and both refund tx arrays are empty, the refund
side effect did not complete and must be handled manually. Take a database
snapshot before any manual edit.

## Status State Machine

- `KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT` means the deposit transaction was
  confirmed and the quote amount passed input bounds checks.
- `PENDING_DEPOSIT -> PROCESSING` means settlement started.
- `PROCESSING -> SUCCESS|FAILED|REFUNDED` are the terminal settlement exits.
- For `FLEX_INPUT`, deposits below the accepted floor become
  `INCOMPLETE_DEPOSIT`.
- For deadline expiry:
  - `PENDING_DEPOSIT` or `KNOWN_DEPOSIT_TX` past deadline becomes `REFUNDED`.
  - `PROCESSING` past deadline is ignored.
  - `INCOMPLETE_DEPOSIT` stays `INCOMPLETE_DEPOSIT`.
  - `REFUNDED` with reason `deadline_expired_no_deposit` means no confirmed
    deposit existed, so no refund tx is expected.

Do not force illegal transitions such as `SUCCESS -> PROCESSING` or
`REFUNDED -> FAILED`. Prefer replaying from the last safe non-terminal state
plus a bridge restart instead of inventing a new status.

## Evidence Capture

Live Sepolia evidence:

```bash
RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 | tee sepolia-e2e-live.log
rg 'SEPOLIA_E2E_EVIDENCE|evidence_report_path|final_status' sepolia-e2e-live.log
docker compose -f compose.sepolia.yaml --env-file .env logs bridge --tail=300
```

Do not claim Sepolia validation unless the run includes live Sepolia tx hashes,
Miden tx ids, and final `SUCCESS` statuses for both inbound and outbound.
