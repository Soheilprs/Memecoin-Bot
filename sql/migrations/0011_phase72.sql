-- Phase 7.2: multi-chain research activation. Descriptive outcomes ≠ execution outcomes.

CREATE TABLE IF NOT EXISTS research_capabilities (
    session_id BIGINT REFERENCES collection_sessions (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    feature_valid BOOLEAN NOT NULL DEFAULT FALSE,
    descriptive_outcome_valid BOOLEAN NOT NULL DEFAULT FALSE,
    execution_valid BOOLEAN NOT NULL DEFAULT FALSE,
    paper_live_valid BOOLEAN NOT NULL DEFAULT FALSE,
    non_research_valid BOOLEAN NOT NULL DEFAULT FALSE,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS prospective_sessions (
    id BIGSERIAL PRIMARY KEY,
    collection_session_id BIGINT REFERENCES collection_sessions (id),
    mode TEXT NOT NULL,
    chain TEXT NOT NULL REFERENCES chains (id),
    launchpad TEXT REFERENCES launchpads (id),
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    start_block BIGINT,
    end_block BIGINT,
    collector_quality TEXT NOT NULL,
    discovery_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    trades_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    state_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    security_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    features_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    execution_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    outcomes_quality TEXT NOT NULL DEFAULT 'PARTIAL',
    feature_version TEXT NOT NULL,
    security_policy_version TEXT NOT NULL,
    candidate_policy_version TEXT NOT NULL,
    strategy_policy_version TEXT NOT NULL,
    execution_model_version TEXT NOT NULL,
    fee_model_version TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS descriptive_token_outcomes (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    token_address TEXT NOT NULL,
    reference_time TIMESTAMPTZ NOT NULL,
    reference_source_price TEXT,
    max_source_price_5m TEXT,
    max_source_price_15m TEXT,
    max_source_price_30m TEXT,
    max_source_price_1h TEXT,
    max_return_bps BIGINT,
    reached_2x BOOLEAN NOT NULL DEFAULT FALSE,
    reached_5x BOOLEAN NOT NULL DEFAULT FALSE,
    reached_10x BOOLEAN NOT NULL DEFAULT FALSE,
    reached_20x BOOLEAN NOT NULL DEFAULT FALSE,
    time_to_2x_ms BIGINT,
    time_to_5x_ms BIGINT,
    time_to_10x_ms BIGINT,
    time_to_20x_ms BIGINT,
    label_quality TEXT NOT NULL,
    source TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS descriptive_outcomes_token
    ON descriptive_token_outcomes (chain, token_address, reference_time);

CREATE TABLE IF NOT EXISTS shadow_orders (
    id BIGSERIAL PRIMARY KEY,
    prospective_session_id BIGINT REFERENCES prospective_sessions (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    decision_time TIMESTAMPTZ NOT NULL,
    side TEXT NOT NULL,
    requested_amount TEXT NOT NULL,
    status TEXT NOT NULL,
    research_valid BOOLEAN NOT NULL DEFAULT FALSE,
    reason TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS wallet_identities (
    id BIGSERIAL PRIMARY KEY,
    evm_address TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS chain_wallet_activity (
    identity_id BIGINT NOT NULL REFERENCES wallet_identities (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    buy_count BIGINT NOT NULL DEFAULT 0,
    sell_count BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (identity_id, chain)
);

CREATE TABLE IF NOT EXISTS wallet_token_activity (
    identity_id BIGINT NOT NULL REFERENCES wallet_identities (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    buy_count BIGINT NOT NULL DEFAULT 0,
    sell_count BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (identity_id, chain, token_address)
);

ALTER TABLE simulated_positions
    ADD COLUMN IF NOT EXISTS remaining_token_amount TEXT;
