# RUNBOOK

This runbook favors concrete commands over abstract guidance. All commands assume you are at the repo root.

## Bridge Isn't Seeing Deposits on Anvil

Symptoms:

- Quote stays in `PENDING_DEPOSIT` after funds were sent to the deposit address
- `GET /v0/status` never advances past `PENDING_DEPOSIT`
- Bridge logs do not show deposit detection or confirmation

Diagnostic commands:

```bash
docker compose ps
docker compose logs bridge --tail=200 | rg "EVM deposit poll failed|failed to read EVM block tip|invalid deposit address|missing token address"
docker compose exec bridge printenv EVM_RPC_URL
docker compose exec anvil cast rpc eth_blockNumber --rpc-url http://localhost:8545
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, deposit_address, status, updated_at
  FROM quotes
  ORDER BY created_at DESC
  LIMIT 10;
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT q.correlation_id, q.deposit_address, q.status, c.evm_deposit_derivation_path
  FROM quotes q
  JOIN chain_artifacts c USING (correlation_id)
  WHERE q.status IN ('PENDING_DEPOSIT', 'KNOWN_DEPOSIT_TX', 'PROCESSING')
  ORDER BY q.created_at DESC;
"
docker compose exec anvil sh -lc '
  for addr in $(jq -r ".usdc,.usdt,.btc" /state/token-addresses.json); do
    echo "token=$addr"
    cast code "$addr" --rpc-url http://localhost:8545
  done
'
```

Remediation:

1. If `EVM_RPC_URL` is wrong inside `bridge`, fix `.env` or Compose overrides and restart only the bridge:
   ```bash
   docker compose restart bridge
   ```
2. If Anvil is healthy but token contracts are missing or `/state/token-addresses.json` is empty, re-run the bootstrap job:
   ```bash
   docker compose up --build --abort-on-container-exit --exit-code-from anvil-init anvil-init
   docker compose restart bridge
   ```
3. If the quote has no `evm_deposit_derivation_path`, the deposit address was not fully persisted. Recreate the quote instead of patching the row by hand.
4. If the derivation path exists but the deposit transaction never landed on Anvil, resend the funds to the quoted deposit address and watch:
   ```bash
   docker compose logs -f bridge
   ```

## Miden RPC Unreachable

Symptoms:

- `/healthz` returns non-200
- Bridge logs show Miden sync or RPC connection failures
- Outbound deposits stop progressing and inbound settlement cannot mint on Miden

Diagnostic commands:

```bash
curl -i http://localhost:8080/healthz
docker compose ps
docker compose logs miden-node --tail=300
docker compose logs bridge --tail=300 | rg "miden|bootstrap|sync failed|rpc"
docker compose exec miden-node sh -lc 'nc -z 127.0.0.1 57291 && echo rpc-port-open'
docker compose exec bridge printenv MIDEN_RPC_URL
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT *
  FROM miden_bootstrap;
"
```

What to check across the bundled Miden services:

- `store`
- `validator`
- `block-producer`
- `rpc`
- `ntx-builder`

The local image runs `miden-node bundled start`, so check the combined `miden-node` logs for those service names:

```bash
docker compose logs miden-node --tail=500 | rg "store|validator|block-producer|rpc|ntx-builder"
```

Safe restart without re-genesis:

```bash
docker compose restart miden-node
docker compose restart bridge
curl -fsS http://localhost:8080/healthz
```

Do not run `make genesis` for an RPC outage alone. `make genesis` is only for first boot or a deliberate full reset. A plain `docker compose restart miden-node bridge` preserves the existing Docker volumes and does not re-genesis the chain.

## Stuck `PROCESSING` Quotes

Symptoms:

- Quote remains in `PROCESSING` for more than 30 minutes
- Watchdog warnings appear in logs
- No terminal `SUCCESS`, `FAILED`, or `REFUNDED` event is recorded

Diagnostic commands:

```bash
docker compose logs bridge --tail=300 | rg "processing quote exceeds watchdog threshold|SETTLEMENT_|SLIPPAGE_"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, status, updated_at
  FROM quotes
  WHERE status = 'PROCESSING'
  ORDER BY updated_at ASC;
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, from_status, to_status, event_kind, reason, created_at
  FROM lifecycle_events
  WHERE correlation_id = '<correlation-id>'
  ORDER BY id;
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, evm_release_tx_hashes, miden_mint_tx_ids, evm_refund_tx_hashes, miden_refund_tx_ids
  FROM chain_artifacts
  WHERE correlation_id = '<correlation-id>';
"
```

Remediation:

1. Confirm whether settlement actually happened on-chain and only the DB is stale, or whether no settlement transaction was ever submitted.
2. If a settlement tx already exists in `miden_mint_tx_ids` or `evm_release_tx_hashes`, restart the bridge to let resume logic re-run settlement idempotently:
   ```bash
   docker compose restart bridge
   ```
3. If no settlement or refund tx exists, keep a human in the loop. There is no auto-fail path for stuck `PROCESSING` quotes by design.
4. Choose one manual action and record it in the incident ticket before touching state:
   - retry by restarting `bridge`
   - fail externally and leave the quote untouched while investigating
   - move the quote to `REFUNDED` only after you have independently executed the refund path
5. After remediation, re-run the SQL above and verify a new lifecycle event was appended.

## User Reports `REFUNDED` but No Refund Tx

Symptoms:

- `/v0/status` reports `REFUNDED`
- User cannot find an origin-chain or Miden refund transaction
- Lifecycle history shows a refund decision, but artifact arrays are empty

Diagnostic commands:

```bash
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, status, deadline, updated_at
  FROM quotes
  WHERE correlation_id = '<correlation-id>';
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT id, from_status, to_status, event_kind, reason, metadata, created_at
  FROM lifecycle_events
  WHERE correlation_id = '<correlation-id>'
  ORDER BY id;
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT evm_deposit_tx_hashes, evm_refund_tx_hashes, miden_refund_tx_ids, idempotency_keys
  FROM chain_artifacts
  WHERE correlation_id = '<correlation-id>';
"
```

Interpretation:

- If the last refund event reason is `deadline_expired_no_deposit`, an empty refund tx list is expected. The quote expired before a confirmed deposit was recorded, so the bridge moved to `REFUNDED` without broadcasting a refund transaction.
- If a confirmed deposit exists in `lifecycle_events` and both refund tx arrays are still empty, the refund side effect did not complete and must be re-fired manually.

Manual re-fire flow:

1. Take a DB backup or snapshot first.
2. Remove the deadline idempotency key so the same deadline-expiry event can execute again:
   ```bash
   docker compose exec postgres psql -U postgres -d miden_bridge -c "
     UPDATE chain_artifacts
     SET idempotency_keys = COALESCE((
       SELECT jsonb_agg(value)
       FROM jsonb_array_elements_text(idempotency_keys) AS value
       WHERE value <> 'lifecycle:deadline_expired:<correlation-id>'
     ), '[]'::jsonb),
         updated_at = NOW()
     WHERE correlation_id = '<correlation-id>';
   "
   ```
3. Move the quote back to the last safe non-terminal state:
   - If the quote has a detection event but no confirmed-deposit event, use `KNOWN_DEPOSIT_TX`:
   ```bash
   docker compose exec postgres psql -U postgres -d miden_bridge -c "
     UPDATE quotes
     SET status = 'KNOWN_DEPOSIT_TX',
         updated_at = NOW()
     WHERE correlation_id = '<correlation-id>';
   "
   ```
   - If the quote already has a confirmed-deposit event, use `PENDING_DEPOSIT`:
   ```bash
   docker compose exec postgres psql -U postgres -d miden_bridge -c "
     UPDATE quotes
     SET status = 'PENDING_DEPOSIT',
         updated_at = NOW()
     WHERE correlation_id = '<correlation-id>';
   "
   ```
4. Restart the bridge so polling and settlement logic resume from the non-terminal state:
   ```bash
   docker compose restart bridge
   ```
5. Re-check `lifecycle_events` and `chain_artifacts` until an `evm_refund_tx_hashes` or `miden_refund_tx_ids` entry appears.

This is intentionally manual. Do not bulk-update multiple quotes at once.

## Status State Machine

Symptoms:

- Status looks inconsistent with the amount sent or the lifecycle history
- A consumer asks why the bridge reported one state before another
- An operator needs to decide whether a manual state edit is valid

Diagnostic commands:

```bash
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT correlation_id, status, deadline, created_at, updated_at
  FROM quotes
  WHERE correlation_id = '<correlation-id>';
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT id, from_status, to_status, event_kind, reason, metadata, created_at
  FROM lifecycle_events
  WHERE correlation_id = '<correlation-id>'
  ORDER BY id;
"
docker compose exec postgres psql -U postgres -d miden_bridge -c "
  SELECT evm_deposit_tx_hashes, evm_release_tx_hashes, miden_mint_tx_ids, evm_refund_tx_hashes, miden_refund_tx_ids
  FROM chain_artifacts
  WHERE correlation_id = '<correlation-id>';
"
```

Rules:

- `KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT` means the deposit transaction was confirmed and the quote amount passed input bounds checks. This is an internal bridge policy, not a spec rule.
- `PENDING_DEPOSIT -> PROCESSING` means settlement started.
- `PROCESSING -> SUCCESS|FAILED|REFUNDED` are the only terminal settlement exits.
- For `FLEX_INPUT`, the bridge accepts deposits down to `min_amount_in * 0.99`. That 1% slop is intentional.
- For `FLEX_INPUT`, deposits below that 1% slop floor become `INCOMPLETE_DEPOSIT`.
- For deadline expiry:
  - `PENDING_DEPOSIT` or `KNOWN_DEPOSIT_TX` past deadline becomes `REFUNDED`
  - `PROCESSING` past deadline is ignored
  - `INCOMPLETE_DEPOSIT` stays `INCOMPLETE_DEPOSIT`
  - `REFUNDED` with reason `deadline_expired_no_deposit` means no confirmed deposit existed, so no refund tx is expected

Remediation:

1. Do not force illegal transitions such as `SUCCESS -> PROCESSING` or `REFUNDED -> FAILED`.
2. If the lifecycle history contradicts the current quote state, prefer replaying from the last safe non-terminal state plus a bridge restart instead of inventing a new status.
3. After any manual edit, re-run the three SQL queries above and confirm the quote is again progressing under normal pollers.
