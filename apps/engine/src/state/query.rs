use chrono::{DateTime, Utc};

use crate::domain::Chain;
use crate::error::Result;
use crate::storage::EventStore;

use super::snapshot::TokenStateSnapshot;

pub async fn get_latest_state<S: EventStore>(
    store: &S,
    chain: Chain,
    token: &str,
) -> Result<Option<TokenStateSnapshot>> {
    store.latest_snapshot(chain, token).await
}

pub async fn get_snapshot_at_or_before<S: EventStore>(
    store: &S,
    chain: Chain,
    token: &str,
    time: DateTime<Utc>,
) -> Result<Option<TokenStateSnapshot>> {
    store.snapshot_at_or_before(chain, token, time).await
}

pub async fn get_milestone_snapshot<S: EventStore>(
    store: &S,
    chain: Chain,
    token: &str,
    age_ms: i64,
) -> Result<Option<TokenStateSnapshot>> {
    store.milestone_snapshot(chain, token, age_ms).await
}

pub async fn get_token_snapshots<S: EventStore>(
    store: &S,
    chain: Chain,
    token: &str,
) -> Result<Vec<TokenStateSnapshot>> {
    store.list_snapshots(chain, token, false).await
}
