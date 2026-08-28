-- Phase 7.5C: durable lifetime entry claim for reentry=false.
-- Application claim happens BEFORE BUY order persist. This table is the
-- authoritative (experiment, chain, token, arm) uniqueness. The filled-BUY
-- unique index is a last-resort safety net for EXP003+ only so EXP002's
-- four preserved duplicate fills are not rewritten.

CREATE TABLE IF NOT EXISTS experiment_arm_entries (
    experiment_id TEXT NOT NULL,
    chain TEXT NOT NULL,
    token_address TEXT NOT NULL,
    strategy_policy_id TEXT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source TEXT NOT NULL DEFAULT 'CLAIM',
    PRIMARY KEY (experiment_id, chain, token_address, strategy_policy_id)
);

CREATE INDEX IF NOT EXISTS experiment_arm_entries_exp
    ON experiment_arm_entries (experiment_id, claimed_at);

CREATE UNIQUE INDEX IF NOT EXISTS simulated_orders_exp003_token_arm_buy
    ON simulated_orders (token_address, policy_id)
    WHERE side = 'BUY'
      AND policy_id LIKE 'PONS_PROSPECTIVE_EXP003%';
