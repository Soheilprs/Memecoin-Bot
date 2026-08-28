-- Phase 7.3: prospective signals, paper orders already in 0008, live descriptive outcomes.

CREATE TABLE IF NOT EXISTS prospective_signals (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    policy_id TEXT NOT NULL,
    decision_time TIMESTAMPTZ NOT NULL,
    enter BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    research_valid_for_alpha BOOLEAN NOT NULL DEFAULT FALSE,
    feature_vector_id BIGINT,
    security_assessment_id BIGINT,
    candidate_state TEXT,
    desired_notional TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS prospective_signals_token
    ON prospective_signals (chain, token_address, policy_id, decision_time);

CREATE TABLE IF NOT EXISTS solana_shard_checkpoints (
    dataset TEXT NOT NULL,
    shard_name TEXT NOT NULL,
    checksum TEXT,
    row_groups_complete INTEGER,
    tokens_affected BIGINT,
    feature_rows_emitted BIGINT,
    label_rows_emitted BIGINT,
    completed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (dataset, shard_name)
);
