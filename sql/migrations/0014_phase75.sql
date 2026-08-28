-- Phase 7.5: locked prospective experiment audit + health.

CREATE TABLE IF NOT EXISTS experiment_audit (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    event TEXT NOT NULL,
    at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS experiment_audit_exp
    ON experiment_audit (experiment_id, id);

CREATE TABLE IF NOT EXISTS experiment_health (
    id BIGSERIAL PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS experiment_health_exp
    ON experiment_health (experiment_id, observed_at DESC);
