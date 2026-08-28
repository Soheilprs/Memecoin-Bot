use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::{CanonicalStatus, Chain, Finality, Launchpad, QualityStatus, TradeSide};

use super::lifecycle::TokenLifecycleState;
use super::market::MarketState;
use super::rolling::RollingWindowSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SnapshotKind {
    Periodic,
    Milestone,
    Lifecycle,
}

impl SnapshotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Periodic => "PERIODIC",
            Self::Milestone => "MILESTONE",
            Self::Lifecycle => "LIFECYCLE",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "PERIODIC" => Self::Periodic,
            "MILESTONE" => Self::Milestone,
            "LIFECYCLE" => Self::Lifecycle,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStateSnapshot {
    pub id: Option<i64>,
    pub chain: Chain,
    pub token_address: String,
    pub launchpad: Launchpad,
    pub snapshot_time: DateTime<Utc>,
    pub age_ms: i64,
    pub snapshot_kind: SnapshotKind,
    pub lifecycle_trigger: Option<String>,
    pub lifecycle_state: TokenLifecycleState,
    pub quote_asset: Option<String>,
    pub buy_count_total: u64,
    pub sell_count_total: u64,
    pub unique_buyers_total: u64,
    pub unique_sellers_total: u64,
    pub buy_quote_volume_raw_total: String,
    pub sell_quote_volume_raw_total: String,
    pub buy_token_volume_raw_total: String,
    pub sell_token_volume_raw_total: String,
    pub creator_buy_count: u64,
    pub creator_sell_count: u64,
    pub creator_buy_quote_raw: String,
    pub creator_sell_quote_raw: String,
    pub last_trade_side: Option<TradeSide>,
    pub last_trade_token_raw: Option<String>,
    pub last_trade_quote_raw: Option<String>,
    pub last_trade_token_decimals: Option<u8>,
    pub last_trade_quote_decimals: Option<u8>,
    pub curve_progress_bps: Option<u32>,
    pub graduation_progress_bps: Option<u32>,
    pub market_state_type: String,
    pub market_state: MarketState,
    pub rolling_5s: RollingWindowSnapshot,
    pub rolling_15s: RollingWindowSnapshot,
    pub rolling_30s: RollingWindowSnapshot,
    pub rolling_60s: RollingWindowSnapshot,
    pub rolling_120s: RollingWindowSnapshot,
    pub rolling_300s: RollingWindowSnapshot,
    pub rolling_900s: RollingWindowSnapshot,
    pub as_of_event_id: Option<String>,
    pub as_of_block: Option<i64>,
    pub as_of_slot: Option<i64>,
    pub as_of_event_order: String,
    pub data_quality: QualityStatus,
    pub source_session_id: Option<i64>,
    pub canonical_status: CanonicalStatus,
    pub finality: Finality,
    pub version: i32,
    pub superseded: bool,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    /// Point-in-time wallet aggregates. All-None means unavailable, not zero.
    #[serde(default)]
    pub wallet: WalletSnapshot,
}

/// Compact wallet stats captured at snapshot time. Missing fields are UNKNOWN.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletSnapshot {
    pub unique_traders_total: Option<u64>,
    pub repeat_buyer_count: Option<u64>,
    pub mean_buys_per_buyer_milli: Option<u64>,
    pub median_buys_per_buyer: Option<u64>,
    pub last_buy_age_ms: Option<i64>,
    pub last_sell_age_ms: Option<i64>,
    pub last_trade_age_ms: Option<i64>,
    pub creator_last_sell_age_ms: Option<i64>,
    pub top_trader_trade_share_bps: Option<u32>,
    pub top_trader_volume_share_bps: Option<u32>,
}

impl WalletSnapshot {
    pub fn is_available(&self) -> bool {
        self.unique_traders_total.is_some()
    }
}

impl TokenStateSnapshot {
    pub fn compute_fingerprint(&self) -> String {
        let payload = serde_json::json!({
            "chain": self.chain.as_str(),
            "token": self.token_address,
            "t": self.snapshot_time.timestamp_millis(),
            "age": self.age_ms,
            "kind": self.snapshot_kind.as_str(),
            "life": self.lifecycle_state.as_str(),
            "buys": self.buy_count_total,
            "sells": self.sell_count_total,
            "ub": self.unique_buyers_total,
            "us": self.unique_sellers_total,
            "bq": self.buy_quote_volume_raw_total,
            "sq": self.sell_quote_volume_raw_total,
            "cb": self.creator_buy_quote_raw,
            "cs": self.creator_sell_quote_raw,
            "curve_bps": self.curve_progress_bps,
            "grad_bps": self.graduation_progress_bps,
            "mkt": self.market_state,
            "r5": self.rolling_5s,
            "r15": self.rolling_15s,
            "r30": self.rolling_30s,
            "r60": self.rolling_60s,
            "as_of": self.as_of_event_id,
            "order": self.as_of_event_order,
            "q": self.data_quality.as_str(),
            "ver": self.version,
        });
        let bytes = serde_json::to_vec(&payload).expect("snapshot fingerprint json");
        hex::encode(Sha256::digest(bytes))
    }
}

use crate::domain::QualityCheck;
use crate::error::DatasetQualityError;

pub fn validate_snapshot_for_simulation(
    snapshot: &TokenStateSnapshot,
    check: QualityCheck,
) -> Result<(), DatasetQualityError> {
    if !check.simulation_requires_complete_market_data {
        return Ok(());
    }
    if !snapshot.data_quality.is_research_complete() {
        metrics::counter!(
            "dataset_quality_rejection_total",
            "reason" => "incomplete_snapshot"
        )
        .increment(1);
        return Err(DatasetQualityError::IncompleteSource {
            chain: snapshot.chain.as_str().to_string(),
            mode: snapshot.data_quality.as_str().to_string(),
            status: snapshot.data_quality.as_str().to_string(),
        });
    }
    Ok(())
}
