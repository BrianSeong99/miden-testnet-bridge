CREATE TABLE chain_artifacts (
    correlation_id UUID PRIMARY KEY REFERENCES quotes(correlation_id) ON DELETE CASCADE,
    evm_deposit_tx_hashes JSONB NOT NULL DEFAULT '[]'::jsonb,
    evm_release_tx_hashes JSONB NOT NULL DEFAULT '[]'::jsonb,
    miden_mint_tx_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    miden_consume_tx_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    evm_refund_tx_hashes JSONB NOT NULL DEFAULT '[]'::jsonb,
    miden_refund_tx_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    intent_hashes JSONB NOT NULL DEFAULT '[]'::jsonb,
    near_tx_hashes JSONB NOT NULL DEFAULT '[]'::jsonb,
    evm_deposit_derivation_path TEXT NULL,
    miden_deposit_account_id TEXT NULL,
    miden_deposit_seed_hex TEXT NULL,
    idempotency_keys JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
