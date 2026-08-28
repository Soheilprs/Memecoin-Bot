use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad, QualityStatus};
use crate::security::assessment::SecurityVerdict;

use super::opt::{OptAmt, OptI64, OptU64};

pub const FEATURE_VERSION: &str = "5.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    pub id: Option<i64>,
    pub chain: Chain,
    pub token_address: String,
    pub launchpad: Launchpad,
    pub snapshot_id: Option<i64>,
    pub security_assessment_id: Option<i64>,
    pub as_of_block: Option<i64>,
    pub as_of_block_hash: Option<String>,
    pub as_of_slot: Option<i64>,
    pub as_of_time: DateTime<Utc>,
    pub token_age_ms: i64,
    pub feature_version: String,
    pub data_quality: QualityStatus,
    pub flow_quality: super::opt::FeatureQuality,
    pub liquidity_quality: super::opt::FeatureQuality,
    pub holder_quality: super::opt::FeatureQuality,
    pub creator_quality: super::opt::FeatureQuality,
    pub shared: SharedFeatures,
    pub protocol: ProtocolFeatures,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFeatures {
    pub token_age_ms: i64,
    pub trade_count_total: u64,
    pub buy_count_total: u64,
    pub sell_count_total: u64,
    pub unique_buyers_total: u64,
    pub unique_sellers_total: u64,
    pub unique_traders_total: OptU64,
    pub buy_quote_volume_total: String,
    pub sell_quote_volume_total: String,
    pub net_quote_flow_total: String,
    pub avg_buy_size: Option<String>,
    pub median_buy_size: OptAmt,
    pub max_buy_size: OptAmt,
    pub avg_sell_size: Option<String>,
    pub median_sell_size: OptAmt,
    pub max_sell_size: OptAmt,
    pub creator_buy_quote_total: String,
    pub creator_sell_quote_total: String,
    pub creator_net_quote_flow: String,
    pub creator_buy_count: u64,
    pub creator_sell_count: u64,
    pub creator_has_sold: bool,
    pub creator_sell_fraction_bps: Option<u32>,
    pub time_since_last_trade_ms: OptI64,
    pub time_since_last_buy_ms: OptI64,
    pub time_since_last_sell_ms: OptI64,
    pub time_since_creator_last_sell_ms: OptI64,
    pub trade_count_imbalance: i64,
    pub buy_sell_count_ratio_bps: Option<u32>,
    pub quote_volume_imbalance: String,
    pub buy_sell_quote_ratio_bps: Option<u32>,
    pub unique_buyer_seller_ratio_bps: Option<u32>,
    pub win5: WindowFeatures,
    pub win15: WindowFeatures,
    pub win30: WindowFeatures,
    pub win60: WindowFeatures,
    pub unique_buyer_velocity_5s: OptI64,
    pub unique_buyer_velocity_15s: OptI64,
    pub unique_buyer_acceleration_5s: OptI64,
    pub unique_buyer_acceleration_15s: OptI64,
    pub unique_buyer_acceleration_30s: OptI64,
    pub unique_seller_velocity_5s: OptI64,
    pub unique_seller_velocity_15s: OptI64,
    pub unique_seller_acceleration_15s: OptI64,
    pub unique_seller_acceleration_30s: OptI64,
    pub buy_volume_velocity_15s: OptAmt,
    pub sell_volume_velocity_15s: OptAmt,
    pub net_flow_velocity_15s: OptAmt,
    pub trades_per_second_5s_milli: OptU64,
    pub trades_per_second_15s_milli: OptU64,
    pub trades_per_second_60s_milli: OptU64,
    pub buy_trades_per_second_15s_milli: OptU64,
    pub sell_trades_per_second_15s_milli: OptU64,
    pub repeat_buyer_count: OptU64,
    pub repeat_buyer_ratio_bps: Option<u32>,
    pub mean_buys_per_buyer_milli: OptU64,
    pub median_buys_per_buyer: OptU64,
    pub new_buyer_ratio_30s_bps: Option<u32>,
    pub trades_per_unique_wallet_milli: OptU64,
    pub top_trader_trade_share_bps: OptU64,
    pub top_trader_volume_share_bps: OptU64,
    pub wash_trade_indicator_count: OptU64,
    pub wash_trade_volume_fraction_bps: OptU64,
    pub wash_adjustment_quality: super::opt::FeatureQuality,
    pub holder_count: OptU64,
    pub top1_pct_bps: OptU64,
    pub top5_pct_bps: OptU64,
    pub top10_pct_bps: OptU64,
    pub top20_pct_bps: OptU64,
    pub creator_pct_bps: OptU64,
    pub cluster_merged_top10_pct_bps: OptU64,
    pub bundle_supply_pct_bps: OptU64,
    pub creator_cluster_supply_pct_bps: OptU64,
    pub liquidity_quote: OptAmt,
    pub liquidity_quality: super::opt::FeatureQuality,
    pub estimated_exit_capacity: OptAmt,
    pub max_notional_at_1pct: OptAmt,
    pub max_notional_at_2pct: OptAmt,
    pub max_notional_at_5pct: OptAmt,
    pub current_price_quote_per_token: OptAmt,
    pub price_change_5s_bps: OptI64,
    pub price_change_15s_bps: OptI64,
    pub price_change_30s_bps: OptI64,
    pub price_change_60s_bps: OptI64,
    pub return_since_discovery_bps: OptI64,
    pub is_pre_graduation: bool,
    pub is_migrating: bool,
    pub is_graduation_gap: bool,
    pub is_post_graduation: bool,
    pub time_since_graduation_ms: OptI64,
    pub current_progress_to_graduation_bps: OptU64,
    pub security_verdict: Option<String>,
    pub contract_risk: Option<String>,
    pub privilege_risk: Option<String>,
    pub sellability_risk: Option<String>,
    pub liquidity_structure_risk: Option<String>,
    pub warning_count: u32,
    pub creator_prior_launches: OptU64,
    pub creator_prior_rugs: OptU64,
    pub hour_of_day_utc: u32,
    pub launches_last_5m: OptU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowFeatures {
    pub duration_ms: i64,
    pub buy_count: u64,
    pub sell_count: u64,
    pub unique_buyers: u64,
    pub unique_sellers: u64,
    pub new_unique_buyers: u64,
    pub new_unique_sellers: u64,
    pub buy_quote_volume: String,
    pub sell_quote_volume: String,
    pub net_quote_flow: String,
    pub median_trade_size: OptAmt,
    pub max_trade_size: OptAmt,
    pub creator_buy_volume: String,
    pub creator_sell_volume: String,
    pub trade_count_imbalance: i64,
    pub buy_sell_count_ratio_bps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum ProtocolFeatures {
    SolanaPump {
        curve_progress_bps: OptU64,
        virtual_quote_reserve: OptAmt,
        real_quote_reserve: OptAmt,
        token_reserve: OptAmt,
        curve_progress_velocity_bps: OptI64,
    },
    RobinhoodPons {
        graduation_progress_bps: OptU64,
        snipe_tax_window_elapsed: OptU64,
    },
    BaseClanker {
        has_pool_id: bool,
        sqrt_price_x96: OptAmt,
        liquidity_raw: OptAmt,
        tick: OptAmt,
    },
    None,
}

impl FeatureVector {
    pub fn content_fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let payload = serde_json::json!({
            "t": self.as_of_time.timestamp_millis(),
            "age": self.token_age_ms,
            "buys": self.shared.buy_count_total,
            "ub": self.shared.unique_buyers_total,
            "bq": self.shared.buy_quote_volume_total,
            "accel15": self.shared.unique_buyer_acceleration_15s,
            "verdict": self.shared.security_verdict,
            "top10": self.shared.top10_pct_bps,
            "liq": self.shared.liquidity_quote,
            "ver": self.feature_version,
        });
        hex::encode(Sha256::digest(serde_json::to_vec(&payload).unwrap()))
    }
}

pub fn security_blocks_eligible(v: Option<SecurityVerdict>) -> bool {
    !matches!(v, Some(SecurityVerdict::Pass) | Some(SecurityVerdict::Warn))
}
