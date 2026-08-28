-- Phase 7.5A: observation intervals + no-reentry uniqueness for prospective arms.

CREATE TABLE IF NOT EXISTS experiment_observation_intervals (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    status TEXT NOT NULL,
    discovery_quality TEXT,
    trade_quality TEXT,
    feature_quality TEXT,
    execution_quality TEXT,
    gap_reason TEXT,
    heartbeat_at TIMESTAMPTZ,
    UNIQUE (experiment_id, started_at)
);

CREATE INDEX IF NOT EXISTS experiment_obs_exp
    ON experiment_observation_intervals (experiment_id, started_at);

CREATE UNIQUE INDEX IF NOT EXISTS simulated_positions_exp_arm_token
    ON simulated_positions (token_address, strategy_policy_id)
    WHERE strategy_policy_id LIKE 'PONS_PROSPECTIVE_EXP%';
