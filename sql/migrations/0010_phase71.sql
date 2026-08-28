-- Phase 7.1: historical dataset registry. Large parquet files stay outside Git.

CREATE TABLE IF NOT EXISTS historical_datasets (
    dataset_id TEXT PRIMARY KEY,
    dataset_hash TEXT NOT NULL,
    source TEXT NOT NULL,
    source_url TEXT,
    publisher TEXT,
    license TEXT,
    importer_version TEXT NOT NULL,
    feature_valid BOOLEAN NOT NULL,
    execution_valid BOOLEAN NOT NULL,
    quality_status TEXT NOT NULL,
    manifest JSONB NOT NULL,
    retrieved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS historical_dataset_imports (
    id BIGSERIAL PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES historical_datasets (dataset_id),
    collection_session_id BIGINT REFERENCES collection_sessions (id),
    subset_label TEXT,
    rows_read BIGINT NOT NULL DEFAULT 0,
    events_emitted BIGINT NOT NULL DEFAULT 0,
    rejected_rows BIGINT NOT NULL DEFAULT 0,
    invalid_rows BIGINT NOT NULL DEFAULT 0,
    duration_ms BIGINT,
    peak_memory_bytes BIGINT,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
