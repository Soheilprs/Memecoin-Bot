-- Phase 7.4: Pons curve state reads + prospective outcome maturity.

CREATE TABLE IF NOT EXISTS pons_curve_states (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    curve TEXT NOT NULL,
    block_number BIGINT NOT NULL,
    block_hash TEXT,
    observed_at TIMESTAMPTZ NOT NULL,
    virtual_quote_reserve TEXT NOT NULL,
    virtual_token_reserve TEXT NOT NULL,
    real_quote_reserve TEXT NOT NULL,
    real_token_reserve TEXT NOT NULL,
    quote_collected TEXT,
    graduation_threshold TEXT,
    progress_bps INTEGER,
    status TEXT NOT NULL,
    fee_bps INTEGER,
    snipe_tax_bps INTEGER,
    creator_tax_bps INTEGER,
    state_quality TEXT NOT NULL,
    source TEXT NOT NULL,
    abi_version TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (curve, block_number)
);

CREATE INDEX IF NOT EXISTS pons_curve_states_token
    ON pons_curve_states (chain, token_address, block_number);

ALTER TABLE descriptive_token_outcomes
    ADD COLUMN IF NOT EXISTS maturity TEXT NOT NULL DEFAULT 'MATURE';
