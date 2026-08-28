-- Phase 4: append-only security assessments. Do not overwrite history.

INSERT INTO launchpads (id, display_name) VALUES ('unknown', 'Unknown')
ON CONFLICT (id) DO NOTHING;

ALTER TABLE contracts
    ADD COLUMN IF NOT EXISTS runtime_bytecode_hash TEXT,
    ADD COLUMN IF NOT EXISTS normalized_hash TEXT,
    ADD COLUMN IF NOT EXISTS proxy_type TEXT,
    ADD COLUMN IF NOT EXISTS upgrade_admin TEXT,
    ADD COLUMN IF NOT EXISTS creator TEXT;

CREATE TABLE IF NOT EXISTS security_assessments (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    snapshot_id BIGINT,
    as_of_block BIGINT,
    as_of_block_hash TEXT,
    as_of_slot BIGINT,
    as_of_time TIMESTAMPTZ NOT NULL,
    verdict TEXT NOT NULL,
    contract_risk TEXT NOT NULL,
    token_mechanics_risk TEXT NOT NULL,
    privilege_risk TEXT NOT NULL,
    sellability_risk TEXT NOT NULL,
    liquidity_structure_risk TEXT NOT NULL,
    hard_reject_reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    warnings JSONB NOT NULL DEFAULT '[]'::jsonb,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    analyzer_version TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    data_quality TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX security_assessments_token
    ON security_assessments (chain, token_address, id DESC);
CREATE INDEX security_assessments_verdict
    ON security_assessments (chain, verdict, created_at DESC);

CREATE TABLE IF NOT EXISTS token_current_security (
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    latest_assessment_id BIGINT REFERENCES security_assessments (id),
    verdict TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chain, token_address)
);
