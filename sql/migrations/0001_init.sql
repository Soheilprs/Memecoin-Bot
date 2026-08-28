CREATE TABLE chains (
    id TEXT PRIMARY KEY,
    chain_id BIGINT,
    name TEXT NOT NULL
);

CREATE TABLE launchpads (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL
);

CREATE TABLE factories (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    address TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    source TEXT NOT NULL,
    abi_idl_version TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (chain, address)
);

CREATE TABLE decoder_artifacts (
    protocol TEXT NOT NULL,
    chain TEXT NOT NULL,
    version TEXT NOT NULL,
    source TEXT NOT NULL,
    retrieved_at TIMESTAMPTZ NOT NULL,
    sha256 TEXT NOT NULL,
    PRIMARY KEY (protocol, chain, version)
);

CREATE TABLE raw_events (
    id TEXT PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    source TEXT NOT NULL,
    block_number BIGINT,
    block_hash TEXT,
    slot BIGINT,
    tx_hash TEXT NOT NULL,
    log_index BIGINT,
    instruction_index INTEGER,
    inner_instruction_index INTEGER,
    payload JSONB NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    persisted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    chain_time TIMESTAMPTZ,
    canonical_status TEXT NOT NULL DEFAULT 'canonical',
    finality TEXT NOT NULL DEFAULT 'unknown',
    decoder_status TEXT NOT NULL DEFAULT 'pending',
    decoder_version TEXT,
    error TEXT,
    removed BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE UNIQUE INDEX raw_events_evm_identity
    ON raw_events (chain, tx_hash, log_index)
    WHERE log_index IS NOT NULL;

CREATE UNIQUE INDEX raw_events_solana_identity
    ON raw_events (
        chain,
        tx_hash,
        instruction_index,
        COALESCE(inner_instruction_index, -1)
    )
    WHERE instruction_index IS NOT NULL;

CREATE INDEX raw_events_chain_slot ON raw_events (chain, slot);
CREATE INDEX raw_events_chain_block ON raw_events (chain, block_number);

CREATE TABLE tokens (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    creator TEXT,
    launchpad TEXT REFERENCES launchpads (id),
    factory_or_program TEXT,
    first_discovered_event_id TEXT REFERENCES raw_events (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chain, token_address)
);

CREATE TABLE token_discovered (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE REFERENCES raw_events (id),
    chain TEXT NOT NULL REFERENCES chains (id),
    chain_id BIGINT,
    token_address TEXT NOT NULL,
    creator TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    factory_or_program TEXT NOT NULL,
    pool TEXT,
    curve TEXT,
    quote_asset TEXT,
    launch_mechanism TEXT NOT NULL,
    bonding_curve BOOLEAN NOT NULL,
    graduation_model TEXT NOT NULL,
    block_number BIGINT,
    block_hash TEXT,
    slot BIGINT,
    tx_hash TEXT NOT NULL,
    instruction_index INTEGER,
    inner_instruction_index INTEGER,
    log_index BIGINT,
    chain_time TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL,
    persisted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source TEXT NOT NULL,
    decoder_version TEXT NOT NULL,
    initial_liquidity TEXT,
    raw_event_id TEXT NOT NULL REFERENCES raw_events (id),
    payload JSONB NOT NULL
);

CREATE INDEX token_discovered_token ON token_discovered (chain, token_address);

CREATE TABLE candidate_states (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL,
    token_address TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'discovered',
    reason TEXT,
    event_id TEXT REFERENCES raw_events (id),
    entered_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE risk_assessments (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL,
    token_address TEXT NOT NULL,
    as_of_event_id TEXT REFERENCES raw_events (id),
    risk_score INTEGER,
    hard_reject BOOLEAN NOT NULL DEFAULT FALSE,
    reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE contracts (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL,
    address TEXT NOT NULL,
    bytecode_hash TEXT,
    factory TEXT,
    implementation TEXT,
    first_seen_event_id TEXT REFERENCES raw_events (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chain, address)
);

CREATE TABLE ingest_checkpoints (
    ingest_id TEXT PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    last_block BIGINT,
    last_block_hash TEXT,
    last_slot BIGINT,
    last_signature TEXT,
    overlap_blocks INTEGER NOT NULL DEFAULT 64,
    overlap_slots INTEGER NOT NULL DEFAULT 32,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO chains (id, chain_id, name) VALUES
    ('solana', NULL, 'Solana'),
    ('base', 8453, 'Base'),
    ('robinhood', 4663, 'Robinhood Chain');

INSERT INTO launchpads (id, display_name) VALUES
    ('pumpfun', 'Pump.fun'),
    ('pons_v2', 'Pons V2'),
    ('clanker_v4', 'Clanker v4'),
    ('unknown', 'Unknown');

INSERT INTO factories (
    chain, launchpad, address, verification_status, source, abi_idl_version, enabled
) VALUES
    (
        'solana',
        'pumpfun',
        '6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P',
        'verified',
        'pump-public-docs IDL + multiple 2026 catalogs',
        '0.1.0',
        TRUE
    ),
    (
        'robinhood',
        'pons_v2',
        '0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e',
        'verified',
        'on-chain bytecode + TokenLaunched logs 2026-08-27',
        'v2-tokenlaunched-1',
        TRUE
    ),
    (
        'base',
        'clanker_v4',
        '0xe85a59c628f7d27878aceb4bf3b35733630083a9',
        'verified',
        'clanker-devco/v4-contracts deployed contracts',
        'v4-tokencreated-1',
        TRUE
    );
