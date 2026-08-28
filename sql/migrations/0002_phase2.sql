-- Phase 2: trades, lifecycle, ingest gaps, chain heads, checkpoints.

INSERT INTO launchpads (id, display_name) VALUES
    ('pumpswap', 'PumpSwap')
ON CONFLICT (id) DO NOTHING;

ALTER TABLE ingest_checkpoints
    ADD COLUMN stream TEXT NOT NULL DEFAULT 'default',
    ADD COLUMN last_seen_block BIGINT,
    ADD COLUMN last_finalized_block BIGINT,
    ADD COLUMN last_seen_slot BIGINT,
    ADD COLUMN last_confirmed_slot BIGINT,
    ADD COLUMN last_finalized_slot BIGINT;

UPDATE ingest_checkpoints SET last_seen_block = last_block WHERE last_seen_block IS NULL;
UPDATE ingest_checkpoints SET last_seen_slot = last_slot WHERE last_seen_slot IS NULL;

ALTER TABLE raw_events ADD COLUMN transaction_index BIGINT;

CREATE TABLE token_trades (
    event_id TEXT PRIMARY KEY REFERENCES raw_events (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    token_id BIGINT REFERENCES tokens (id),
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    trader TEXT NOT NULL,
    side TEXT NOT NULL,
    base_amount_raw TEXT NOT NULL,
    quote_amount_raw TEXT NOT NULL,
    base_decimals INTEGER NOT NULL,
    quote_decimals INTEGER NOT NULL,
    quote_asset TEXT NOT NULL,
    pool TEXT,
    curve TEXT,
    price_estimate TEXT,
    block_number BIGINT,
    block_hash TEXT,
    slot BIGINT,
    transaction_index BIGINT,
    tx_hash TEXT NOT NULL,
    log_index BIGINT,
    instruction_index INTEGER,
    inner_instruction_index INTEGER,
    chain_time TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL,
    persisted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    canonical_status TEXT NOT NULL DEFAULT 'canonical',
    finality TEXT NOT NULL DEFAULT 'unknown',
    source TEXT NOT NULL,
    decoder_version TEXT NOT NULL,
    raw_event_id TEXT NOT NULL REFERENCES raw_events (id),
    payload JSONB NOT NULL
);

CREATE INDEX token_trades_token_time
    ON token_trades (chain, token_address, block_number, slot, transaction_index, log_index, instruction_index);
CREATE INDEX token_trades_chain_block ON token_trades (chain, block_number);
CREATE INDEX token_trades_chain_slot ON token_trades (chain, slot);
CREATE INDEX token_trades_trader ON token_trades (chain, trader);
CREATE INDEX token_trades_launchpad_time ON token_trades (launchpad, chain_time);

CREATE TABLE lifecycle_events (
    event_id TEXT PRIMARY KEY REFERENCES raw_events (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    token_id BIGINT REFERENCES tokens (id),
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    type TEXT NOT NULL,
    factory TEXT,
    pool TEXT,
    curve TEXT,
    block_number BIGINT,
    block_hash TEXT,
    slot BIGINT,
    transaction_index BIGINT,
    tx_hash TEXT NOT NULL,
    log_index BIGINT,
    instruction_index INTEGER,
    inner_instruction_index INTEGER,
    chain_time TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL,
    persisted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    canonical_status TEXT NOT NULL DEFAULT 'canonical',
    finality TEXT NOT NULL DEFAULT 'unknown',
    source TEXT NOT NULL,
    decoder_version TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    raw_event_id TEXT NOT NULL REFERENCES raw_events (id),
    payload JSONB NOT NULL
);

CREATE INDEX lifecycle_events_token_time
    ON lifecycle_events (chain, token_address, block_number, slot, log_index, instruction_index);
CREATE INDEX lifecycle_events_chain_block ON lifecycle_events (chain, block_number);
CREATE INDEX lifecycle_events_chain_slot ON lifecycle_events (chain, slot);
CREATE INDEX lifecycle_events_launchpad_time ON lifecycle_events (launchpad, chain_time);
CREATE INDEX lifecycle_events_type ON lifecycle_events (chain, type);

CREATE TABLE ingest_gaps (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    source TEXT NOT NULL,
    stream TEXT NOT NULL,
    from_block BIGINT,
    to_block BIGINT,
    from_slot BIGINT,
    to_slot BIGINT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    recovered BOOLEAN NOT NULL DEFAULT FALSE,
    recovered_at TIMESTAMPTZ,
    reason TEXT NOT NULL
);

CREATE INDEX ingest_gaps_chain ON ingest_gaps (chain, recovered, detected_at);

CREATE TABLE chain_heads (
    chain TEXT PRIMARY KEY REFERENCES chains (id),
    latest_block BIGINT,
    latest_block_hash TEXT,
    latest_slot BIGINT,
    finalized_block BIGINT,
    finalized_slot BIGINT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lag_ms BIGINT
);

CREATE TABLE watched_markets (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    token_address TEXT NOT NULL,
    pool TEXT,
    curve TEXT,
    pool_id TEXT,
    hook TEXT,
    source_event_id TEXT REFERENCES raw_events (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chain, token_address)
);

CREATE INDEX watched_markets_pool ON watched_markets (chain, pool);
CREATE INDEX watched_markets_pool_id ON watched_markets (chain, pool_id);
CREATE INDEX watched_markets_curve ON watched_markets (chain, curve);

INSERT INTO factories (
    chain, launchpad, address, verification_status, source, abi_idl_version, enabled
) VALUES
    (
        'solana',
        'pumpswap',
        'pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA',
        'verified',
        'pump-public-docs PumpSwap program',
        '0.1.0',
        TRUE
    ),
    (
        'robinhood',
        'pons_v2',
        '0x8366a39cc670b4001a1121b8f6a443a643e40951',
        'verified',
        'Robinhood Uniswap v4 PoolManager (Pons post-grad venue)',
        'v4-poolmanager-1',
        TRUE
    ),
    (
        'base',
        'clanker_v4',
        '0x498581ff718922c3f8e6a244956af099b2652b2b',
        'verified',
        'Base Uniswap v4 PoolManager (Clanker v4 venue)',
        'v4-poolmanager-1',
        TRUE
    )
ON CONFLICT (chain, address) DO NOTHING;
