CREATE TABLE lifecycle_events (
    id BIGSERIAL PRIMARY KEY,
    correlation_id UUID NOT NULL REFERENCES quotes(correlation_id) ON DELETE CASCADE,
    from_status TEXT NULL,
    to_status TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    reason TEXT NULL,
    metadata JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX lifecycle_events_correlation_id_idx
    ON lifecycle_events (correlation_id);
