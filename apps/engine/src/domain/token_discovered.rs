use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::chain::Chain;
use super::launchpad::{GraduationModel, LaunchMechanism, Launchpad};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDiscovered {
    pub chain: Chain,
    pub chain_id: Option<u64>,
    pub token_address: String,
    pub creator: String,
    pub launchpad: Launchpad,
    pub factory_or_program: String,
    pub pool: Option<String>,
    pub curve: Option<String>,
    pub quote_asset: Option<String>,
    pub launch_mechanism: LaunchMechanism,
    pub bonding_curve: bool,
    pub graduation_model: GraduationModel,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub slot: Option<u64>,
    pub tx_hash_or_signature: String,
    pub instruction_index: Option<u32>,
    pub inner_instruction_index: Option<u32>,
    pub log_index: Option<u64>,
    pub chain_timestamp: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub persisted_at: Option<DateTime<Utc>>,
    pub source: String,
    pub decoder_version: String,
    pub initial_liquidity: Option<String>,
    pub raw_event_id: String,
}

impl TokenDiscovered {
    pub fn canonical_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("TokenDiscovered is serializable")
    }
}
