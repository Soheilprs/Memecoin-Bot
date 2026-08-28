//! JSON-RPC getTransaction adapter. Primary live path is Yellowstone gRPC.

use chrono::{DateTime, Utc};

use crate::domain::{Finality, RawEvent};

use super::tx::{raw_events_from_view, view_from_get_transaction};

pub fn raw_events_from_get_transaction(
    tx: &serde_json::Value,
    source: &str,
    observed_at: DateTime<Utc>,
    finality: Finality,
) -> Vec<RawEvent> {
    match view_from_get_transaction(tx, source, observed_at, finality) {
        Some(view) => raw_events_from_view(&view),
        None => Vec::new(),
    }
}
