use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::chain::Chain;
use super::launchpad::Launchpad;
use super::raw_event::{CanonicalStatus, Finality};
use super::trade::EventOrderKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleType {
    TokenCreated,
    MigrationStarted,
    Migrated,
    CurveCompleted,
    LaunchSwept,
    PoolCreated,
    PoolGraduated,
    SnipeTaxCharged,
}

impl LifecycleType {
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleType::TokenCreated => "TOKEN_CREATED",
            LifecycleType::MigrationStarted => "MIGRATION_STARTED",
            LifecycleType::Migrated => "MIGRATED",
            LifecycleType::CurveCompleted => "CURVE_COMPLETED",
            LifecycleType::LaunchSwept => "LAUNCH_SWEPT",
            LifecycleType::PoolCreated => "POOL_CREATED",
            LifecycleType::PoolGraduated => "POOL_GRADUATED",
            LifecycleType::SnipeTaxCharged => "SNIPE_TAX_CHARGED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "TOKEN_CREATED" | "token_created" => Some(LifecycleType::TokenCreated),
            "MIGRATION_STARTED" | "migration_started" => Some(LifecycleType::MigrationStarted),
            "MIGRATED" | "migrated" => Some(LifecycleType::Migrated),
            "CURVE_COMPLETED" | "curve_completed" => Some(LifecycleType::CurveCompleted),
            "LAUNCH_SWEPT" | "launch_swept" => Some(LifecycleType::LaunchSwept),
            "POOL_CREATED" | "pool_created" => Some(LifecycleType::PoolCreated),
            "POOL_GRADUATED" | "pool_graduated" => Some(LifecycleType::PoolGraduated),
            "SNIPE_TAX_CHARGED" | "snipe_tax_charged" => Some(LifecycleType::SnipeTaxCharged),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleObserved {
    pub event_id: String,
    pub chain: Chain,
    pub launchpad: Launchpad,
    pub token_address: String,
    pub lifecycle_type: LifecycleType,
    pub factory: Option<String>,
    pub pool: Option<String>,
    pub curve: Option<String>,
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

impl LifecycleObserved {
    pub fn canonical_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("LifecycleObserved is serializable")
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
