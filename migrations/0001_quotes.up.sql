CREATE TABLE quotes (
    correlation_id UUID PRIMARY KEY,
    deposit_address TEXT NOT NULL,
    deposit_memo TEXT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'KNOWN_DEPOSIT_TX',
            'PENDING_DEPOSIT',
            'INCOMPLETE_DEPOSIT',
            'PROCESSING',
            'SUCCESS',
            'REFUNDED',
            'FAILED'
        )
    ),
    quote_request_json JSONB NOT NULL,
    quote_response_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deadline TIMESTAMPTZ NULL
);

CREATE INDEX quotes_deposit_address_deposit_memo_idx
    ON quotes (deposit_address, deposit_memo);
