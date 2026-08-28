-- Phase 2.1: Solana stream health, PumpSwap market continuity, failed-tx metadata.

ALTER TABLE ingest_checkpoints
    ADD COLUMN last_received_slot BIGINT,
    ADD COLUMN last_persisted_slot BIGINT;

ALTER TABLE raw_events
    ADD COLUMN execution_status TEXT NOT NULL DEFAULT 'success';

ALTER TABLE watched_markets
    ADD COLUMN source_curve TEXT,
    ADD COLUMN destination_dex TEXT,
    ADD COLUMN quote_asset TEXT,
    ADD COLUMN migration_slot BIGINT,
    ADD COLUMN registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN active BOOLEAN NOT NULL DEFAULT TRUE;

CREATE INDEX IF NOT EXISTS lifecycle_events_token_slot
    ON lifecycle_events (chain, token_address, slot, transaction_index, instruction_index);
CREATE INDEX IF NOT EXISTS token_trades_pool_slot
    ON token_trades (chain, pool, slot, transaction_index);
CREATE INDEX IF NOT EXISTS lifecycle_events_type_slot
    ON lifecycle_events (chain, type, slot);
CREATE INDEX IF NOT EXISTS watched_markets_active_pool
    ON watched_markets (chain, active, pool)
    WHERE active AND pool IS NOT NULL;
