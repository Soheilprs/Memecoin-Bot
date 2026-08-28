-- Phase 5: point-in-time feature vectors + append-only candidate transitions.
-- Never overwrite historical vectors or transitions. Parallel policies share PK (chain, token, policy_id).

CREATE TABLE IF NOT EXISTS feature_vectors (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    snapshot_id BIGINT REFERENCES token_state_snapshots (id),
    security_assessment_id BIGINT REFERENCES security_assessments (id),
    as_of_block BIGINT,
    as_of_block_hash TEXT,
    as_of_slot BIGINT,
    as_of_time TIMESTAMPTZ NOT NULL,
    token_age_ms BIGINT NOT NULL,
    feature_version TEXT NOT NULL,
    data_quality TEXT NOT NULL,
    flow_quality TEXT NOT NULL,
    liquidity_quality TEXT NOT NULL,
    holder_quality TEXT NOT NULL,
    creator_quality TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX feature_vectors_token_time
    ON feature_vectors (chain, token_address, as_of_time);
CREATE INDEX feature_vectors_version
    ON feature_vectors (feature_version, as_of_time);
CREATE INDEX feature_vectors_snapshot
    ON feature_vectors (snapshot_id);

CREATE TABLE IF NOT EXISTS token_current_features (
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    latest_vector_id BIGINT REFERENCES feature_vectors (id),
    feature_version TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chain, token_address)
);

CREATE TABLE IF NOT EXISTS candidate_state_transitions (
    id BIGSERIAL PRIMARY KEY,
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    launchpad TEXT NOT NULL REFERENCES launchpads (id),
    policy_id TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    reason TEXT NOT NULL,
    as_of_time TIMESTAMPTZ NOT NULL,
    snapshot_id BIGINT REFERENCES token_state_snapshots (id),
    security_assessment_id BIGINT REFERENCES security_assessments (id),
    feature_vector_id BIGINT REFERENCES feature_vectors (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL
);

CREATE INDEX candidate_transitions_token_policy
    ON candidate_state_transitions (chain, token_address, policy_id, id);
CREATE INDEX candidate_transitions_state
    ON candidate_state_transitions (to_state, as_of_time);
CREATE INDEX candidate_transitions_policy
    ON candidate_state_transitions (policy_id, policy_version, as_of_time);

CREATE TABLE IF NOT EXISTS token_current_candidate (
    chain TEXT NOT NULL REFERENCES chains (id),
    token_address TEXT NOT NULL,
    policy_id TEXT NOT NULL,
    latest_transition_id BIGINT REFERENCES candidate_state_transitions (id),
    state TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chain, token_address, policy_id)
);
