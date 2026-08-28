-- Phase 7: experiment registry + simulation experiment_id. Outcomes stay out of feature_vectors.

ALTER TABLE simulation_runs
    ADD COLUMN IF NOT EXISTS experiment_id TEXT;

CREATE INDEX IF NOT EXISTS simulation_runs_experiment
    ON simulation_runs (experiment_id);

CREATE TABLE IF NOT EXISTS strategy_experiments (
    experiment_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    hypothesis TEXT,
    dataset_id TEXT,
    dataset_hash TEXT,
    chain TEXT REFERENCES chains (id),
    launchpad TEXT REFERENCES launchpads (id),
    train_start TIMESTAMPTZ,
    train_end TIMESTAMPTZ,
    validation_start TIMESTAMPTZ,
    validation_end TIMESTAMPTZ,
    test_start TIMESTAMPTZ,
    test_end TIMESTAMPTZ,
    feature_version TEXT NOT NULL,
    security_policy_version TEXT NOT NULL,
    candidate_policy_version TEXT NOT NULL,
    strategy_policy_version TEXT NOT NULL,
    execution_model_version TEXT NOT NULL,
    fee_model_version TEXT NOT NULL,
    impact_model_version TEXT NOT NULL,
    slippage_model_version TEXT NOT NULL,
    outcome_model_version TEXT NOT NULL,
    position_size_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    exit_policy_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    config_hash TEXT,
    locked_config JSONB,
    status TEXT NOT NULL,
    variants_evaluated INTEGER NOT NULL DEFAULT 0,
    hypotheses_tested INTEGER NOT NULL DEFAULT 8,
    git_commit TEXT,
    seed BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS experiment_splits (
    experiment_id TEXT NOT NULL REFERENCES strategy_experiments (experiment_id),
    split TEXT NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    token_count INTEGER,
    PRIMARY KEY (experiment_id, split)
);

CREATE TABLE IF NOT EXISTS experiment_results (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL REFERENCES strategy_experiments (experiment_id),
    split TEXT NOT NULL,
    simulation_run_id BIGINT REFERENCES simulation_runs (id),
    policy_id TEXT NOT NULL,
    research_valid BOOLEAN NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS moonshot_analysis (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL REFERENCES strategy_experiments (experiment_id),
    split TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS feature_cohort_stats (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL REFERENCES strategy_experiments (experiment_id),
    split TEXT NOT NULL,
    feature_name TEXT NOT NULL,
    observation_age_ms BIGINT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
