CREATE TABLE miden_bootstrap (
    singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key = TRUE),
    solver_account_id TEXT NOT NULL,
    eth_faucet_account_id TEXT NOT NULL,
    usdc_faucet_account_id TEXT NOT NULL,
    usdt_faucet_account_id TEXT NOT NULL,
    btc_faucet_account_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
