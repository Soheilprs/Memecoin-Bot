-- Phase 3: historical token state snapshots. Never overwrite history.

CREATE TABLE token_state_snapshots (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    snapshot_time TIMESTAMPTZ NOT NULL,
    age_ms BIGINT NOT NULL,
    snapshot_kind TEXT NOT NULL,
    lifecycle_trigger TEXT,
    lifecycle_state TEXT NOT NULL,
    quote_asset TEXT,
    buy_count_total BIGINT NOT NULL DEFAULT 0,
    sell_count_total BIGINT NOT NULL DEFAULT 0,
    unique_buyers_total BIGINT NOT NULL DEFAULT 0,
    unique_sellers_total BIGINT NOT NULL DEFAULT 0,
    buy_quote_volume_raw_total TEXT NOT NULL DEFAULT '0',
    sell_quote_volume_raw_total TEXT NOT NULL DEFAULT '0',
    buy_token_volume_raw_total TEXT NOT NULL DEFAULT '0',
    sell_token_volume_raw_total TEXT NOT NULL DEFAULT '0',
    creator_buy_count BIGINT NOT NULL DEFAULT 0,
    creator_sell_count BIGINT NOT NULL DEFAULT 0,
    creator_buy_quote_raw TEXT NOT NULL DEFAULT '0',
    creator_sell_quote_raw TEXT NOT NULL DEFAULT '0',
    last_trade_side TEXT,
    last_trade_token_raw TEXT,
    last_trade_quote_raw TEXT,
    last_trade_token_decimals INTEGER,
    last_trade_quote_decimals INTEGER,
    curve_progress_bps INTEGER,
    graduation_progress_bps INTEGER,
    market_state_type TEXT NOT NULL,
    market_state_json JSONB NOT NULL,
    rolling_5s_json JSONB NOT NULL,
    rolling_15s_json JSONB NOT NULL,
    rolling_30s_json JSONB NOT NULL,
    rolling_60s_json JSONB NOT NULL,
    rolling_120s_json JSONB NOT NULL,
    rolling_300s_json JSONB NOT NULL,
    rolling_900s_json JSONB NOT NULL,
    as_of_event_id TEXT,
    as_of_block BIGINT,
    as_of_slot BIGINT,
    as_of_event_order TEXT NOT NULL,
    data_quality TEXT NOT NULL,
    source_session_id BIGINT REFERENCES collection_sessions (id),
    canonical_status TEXT NOT NULL DEFAULT 'canonical',
    finality TEXT NOT NULL DEFAULT 'confirmed',
    version INTEGER NOT NULL DEFAULT 1,
    superseded BOOLEAN NOT NULL DEFAULT FALSE,
    fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX token_state_snapshots_token_time
    ON token_state_snapshots (chain, token_address, snapshot_time);
CREATE INDEX token_state_snapshots_milestone
    ON token_state_snapshots (chain, token_address, snapshot_kind, age_ms)
    WHERE NOT superseded;
CREATE INDEX token_state_snapshots_canonical
    ON token_state_snapshots (chain, token_address, superseded, version);
CREATE INDEX token_state_snapshots_session
    ON token_state_snapshots (source_session_id);

CREATE TABLE token_current_state (
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    latest_snapshot_id BIGINT REFERENCES token_state_snapshots (id),
    lifecycle_state TEXT NOT NULL,
    last_event_time TIMESTAMPTZ,
    last_event_id TEXT,
    data_quality TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chain, token_address)
);
