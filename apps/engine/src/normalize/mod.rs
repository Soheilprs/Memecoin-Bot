use crate::domain::{RawEvent, TokenDiscovered};

pub fn attach_raw_timestamps(raw: &RawEvent, token: &mut TokenDiscovered) {
    token.observed_at = raw.observed_at;
    token.persisted_at = raw.persisted_at;
    if token.chain_timestamp.is_none() {
        token.chain_timestamp = raw.chain_time();
    }
    if token.block_number.is_none() {
        token.block_number = raw.block_number().map(|v| v as u64);
    }
    if token.block_hash.is_none() {
        token.block_hash = raw.block_hash().map(|s| s.to_string());
    }
    if token.slot.is_none() {
        token.slot = raw.slot().map(|v| v as u64);
    }
    token.raw_event_id = raw.event_id();
    token.source = raw.source.clone();
}

pub fn discovery_lag_ms(token: &TokenDiscovered) -> Option<i64> {
    let chain = token.chain_timestamp?;
    Some(
        token
            .observed_at
            .signed_duration_since(chain)
            .num_milliseconds(),
    )
}

pub fn persist_lag_ms(token: &TokenDiscovered) -> Option<i64> {
    let persisted = token.persisted_at?;
    Some(
        persisted
            .signed_duration_since(token.observed_at)
            .num_milliseconds(),
    )
}
