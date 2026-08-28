-- Phase 7.5B: reconstructable paper exit lifecycle (orders, attempts, events).

ALTER TABLE simulated_orders
    ADD COLUMN IF NOT EXISTS position_id BIGINT REFERENCES simulated_positions (id),
    ADD COLUMN IF NOT EXISTS experiment_id TEXT,
    ADD COLUMN IF NOT EXISTS exit_reason TEXT;

CREATE INDEX IF NOT EXISTS simulated_orders_position
    ON simulated_orders (position_id);
CREATE INDEX IF NOT EXISTS simulated_orders_experiment
    ON simulated_orders (experiment_id);
CREATE INDEX IF NOT EXISTS simulated_orders_side_status
    ON simulated_orders (side, status);

ALTER TABLE execution_attempts
    ADD COLUMN IF NOT EXISTS experiment_id TEXT,
    ADD COLUMN IF NOT EXISTS position_id BIGINT REFERENCES simulated_positions (id),
    ADD COLUMN IF NOT EXISTS chain TEXT,
    ADD COLUMN IF NOT EXISTS token_address TEXT,
    ADD COLUMN IF NOT EXISTS side TEXT,
    ADD COLUMN IF NOT EXISTS decision_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS block_number BIGINT,
    ADD COLUMN IF NOT EXISTS block_hash TEXT,
    ADD COLUMN IF NOT EXISTS curve_state_id BIGINT REFERENCES pons_curve_states (id),
    ADD COLUMN IF NOT EXISTS requested_token_amount TEXT,
    ADD COLUMN IF NOT EXISTS filled_token_amount TEXT,
    ADD COLUMN IF NOT EXISTS quote_received TEXT,
    ADD COLUMN IF NOT EXISTS effective_fill_price TEXT,
    ADD COLUMN IF NOT EXISTS price_impact_bps INTEGER,
    ADD COLUMN IF NOT EXISTS slippage_bps INTEGER,
    ADD COLUMN IF NOT EXISTS protocol_fee TEXT,
    ADD COLUMN IF NOT EXISTS creator_tax TEXT,
    ADD COLUMN IF NOT EXISTS snipe_tax TEXT,
    ADD COLUMN IF NOT EXISTS execution_quality TEXT,
    ADD COLUMN IF NOT EXISTS curve_state_quality TEXT,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT;

CREATE INDEX IF NOT EXISTS execution_attempts_position
    ON execution_attempts (position_id);
CREATE INDEX IF NOT EXISTS execution_attempts_order
    ON execution_attempts (order_id);

ALTER TABLE simulated_positions
    ADD COLUMN IF NOT EXISTS experiment_id TEXT,
    ADD COLUMN IF NOT EXISTS realized_pnl_quote TEXT,
    ADD COLUMN IF NOT EXISTS initial_token_amount TEXT;

CREATE INDEX IF NOT EXISTS simulated_positions_experiment
    ON simulated_positions (experiment_id);

CREATE INDEX IF NOT EXISTS position_events_position
    ON position_events (position_id, at);
