use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::chain::Chain;
use super::launchpad::Launchpad;
use super::raw_event::{CanonicalStatus, Finality};

/// Exact integer token amount as a decimal string. Never f64.
pub type RawAmount = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    pub fn as_str(self) -> &'static str {
        match self {
            TradeSide::Buy => "buy",
            TradeSide::Sell => "sell",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "buy" => Some(TradeSide::Buy),
            "sell" => Some(TradeSide::Sell),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeObserved {
    pub event_id: String,
    pub chain: Chain,
    pub launchpad: Launchpad,
    pub token_address: String,
    pub trader: String,
    pub side: TradeSide,
    pub base_amount_raw: RawAmount,
    pub quote_amount_raw: RawAmount,
    pub base_decimals: u8,
    pub quote_decimals: u8,
    pub quote_asset: String,
    pub pool: Option<String>,
    pub curve: Option<String>,
    /// Integer quote/base ratio in 1e18 fixed-point when safely derivable; otherwise None.
    pub price_estimate: Option<RawAmount>,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub slot: Option<u64>,
    pub transaction_index: Option<u64>,
    pub tx_hash_or_signature: String,
    pub log_index: Option<u64>,
    pub instruction_index: Option<u32>,
    pub inner_instruction_index: Option<u32>,
    pub chain_timestamp: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub persisted_at: Option<DateTime<Utc>>,
    pub canonical_status: CanonicalStatus,
    pub finality: Finality,
    pub source: String,
    pub decoder_version: String,
    pub raw_event_id: String,
    pub metadata: serde_json::Value,
}

impl TradeObserved {
    pub fn canonical_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("TradeObserved is serializable")
    }

    pub fn order_key(&self) -> EventOrderKey {
        EventOrderKey {
            chain: self.chain,
            block_or_slot: self.block_number.or(self.slot).unwrap_or(0),
            transaction_index: self.transaction_index.unwrap_or(0),
            log_or_ix: self
                .log_index
                .unwrap_or(self.instruction_index.map(|v| v as u64).unwrap_or(0)),
            inner: self
                .inner_instruction_index
                .map(|v| v as u64)
                .unwrap_or(u64::MAX),
            event_id: self.event_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventOrderKey {
    pub chain: Chain,
    pub block_or_slot: u64,
    pub transaction_index: u64,
    pub log_or_ix: u64,
    pub inner: u64,
    pub event_id: String,
}
