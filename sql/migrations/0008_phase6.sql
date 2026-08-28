-- Phase 6: simulation runs, orders, attempts, positions, outcomes.
-- Append-only. Outcomes live separately from feature_vectors (no leakage).

CREATE TABLE IF NOT EXISTS simulation_runs (
    id BIGSERIAL PRIMARY KEY,
    mode TEXT NOT NULL,
    chain TEXT REFERENCES chains (id),
    launchpad TEXT REFERENCES launchpads (id),
    strategy_policy_id TEXT NOT NULL,
    strategy_policy_version TEXT NOT NULL,
    execution_model_version TEXT NOT NULL,
    fee_model_version TEXT NOT NULL,
    impact_model_version TEXT NOT NULL,
    failure_model_version TEXT NOT NULL,
    source_session_id BIGINT REFERENCES collection_sessions (id),
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    data_quality TEXT NOT NULL,
    research_valid BOOLEAN NOT NULL,
    config_snapshot JSONB NOT NULL,
    random_seed BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX simulation_runs_policy ON simulation_runs (strategy_policy_id, created_at);

CREATE TABLE IF NOT EXISTS simulated_orders (
    id BIGSERIAL PRIMARY KEY,
    simulation_run_id BIGINT REFERENCES simulation_runs (id),
    policy_id TEXT NOT NULL,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    side TEXT NOT NULL,
    decision_time TIMESTAMPTZ NOT NULL,
    requested_amount TEXT NOT NULL,
    status TEXT NOT NULL,
    feature_vector_id BIGINT,
    security_assessment_id BIGINT,
    candidate_transition_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX simulated_orders_run ON simulated_orders (simulation_run_id, id);
CREATE INDEX simulated_orders_token ON simulated_orders (chain, token_address, decision_time);

CREATE TABLE IF NOT EXISTS execution_attempts (
    id BIGSERIAL PRIMARY KEY,
    simulation_run_id BIGINT REFERENCES simulation_runs (id),
    order_id BIGINT REFERENCES simulated_orders (id),
    attempt_number INTEGER NOT NULL,
    status TEXT NOT NULL,
    eligible_time TIMESTAMPTZ NOT NULL,
    fill_time TIMESTAMPTZ,
    filled_quote TEXT,
    filled_token TEXT,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS simulated_positions (
    id BIGSERIAL PRIMARY KEY,
    simulation_run_id BIGINT REFERENCES simulation_runs (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    strategy_policy_id TEXT NOT NULL,
    opened_at TIMESTAMPTZ NOT NULL,
    closed_at TIMESTAMPTZ,
    status TEXT NOT NULL,
    quote_cost TEXT NOT NULL,
    realized_quote TEXT NOT NULL,
    mfe_quote TEXT,
    mae_quote TEXT,
    capture_ratio_bps INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX simulated_positions_run ON simulated_positions (simulation_run_id, id);

CREATE TABLE IF NOT EXISTS position_events (
    id BIGSERIAL PRIMARY KEY,
    position_id BIGINT NOT NULL REFERENCES simulated_positions (id),
    kind TEXT NOT NULL,
    at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS token_outcomes (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    reference_time TIMESTAMPTZ NOT NULL,
    horizon_ms BIGINT NOT NULL,
    max_return_bps BIGINT,
    final_return_bps BIGINT,
    reached_2x BOOLEAN NOT NULL,
    reached_5x BOOLEAN NOT NULL,
    reached_10x BOOLEAN NOT NULL,
    reached_20x BOOLEAN NOT NULL,
    time_to_2x_ms BIGINT,
    time_to_5x_ms BIGINT,
    time_to_10x_ms BIGINT,
    time_to_20x_ms BIGINT,
    outcome_quality TEXT NOT NULL,
    outcome_model_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX token_outcomes_token ON token_outcomes (chain, token_address, reference_time);

CREATE TABLE IF NOT EXISTS policy_performance (
    id BIGSERIAL PRIMARY KEY,
    simulation_run_id BIGINT REFERENCES simulation_runs (id),
    policy_id TEXT NOT NULL,
    n_orders INTEGER NOT NULL,
    filled_entries INTEGER NOT NULL,
    research_valid BOOLEAN NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
