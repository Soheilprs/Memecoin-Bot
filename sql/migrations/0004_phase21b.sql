-- Phase 2.1B: collection session quality labels. rpc_dev must never look complete.

CREATE TABLE collection_sessions (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    mode TEXT NOT NULL,
    provider TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    start_block BIGINT,
    end_block BIGINT,
    start_slot BIGINT,
    end_slot BIGINT,
    complete BOOLEAN NOT NULL DEFAULT FALSE,
    quality_status TEXT NOT NULL,
    gap_count INTEGER NOT NULL DEFAULT 0,
    notes TEXT
);

CREATE INDEX collection_sessions_chain_started
    ON collection_sessions (chain, started_at DESC);

CREATE INDEX collection_sessions_quality
    ON collection_sessions (chain, quality_status, complete);
