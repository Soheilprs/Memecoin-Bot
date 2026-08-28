use chrono::{DateTime, Utc};

use crate::domain::{QualityStatus, TokenDiscovered};
use crate::state::lifecycle::TokenLifecycleState;
use crate::state::TokenStateSnapshot;

/// Inputs the analyzer may use. Missing historical state is UNKNOWN, never "use now".
#[derive(Debug, Clone, Default)]
pub struct EvmView {
    pub runtime_bytecode: Option<Vec<u8>>,
    pub storage: Vec<(String, String)>,
    pub implementation_bytecode: Option<Vec<u8>>,
    /// When true, bytecode/storage is as-of the requested block. When false and historical,
    /// analyzers must not treat this as historical proof.
    pub as_of_requested_block: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SolanaView {
    pub mint_account: Option<Vec<u8>>,
    pub token_program: Option<String>,
    pub as_of_requested_slot: bool,
}

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub token: TokenDiscovered,
    pub snapshot: Option<TokenStateSnapshot>,
    pub lifecycle: Option<TokenLifecycleState>,
    pub data_quality: QualityStatus,
    pub historical: bool,
    pub as_of_block: Option<u64>,
    pub as_of_slot: Option<u64>,
    pub as_of_time: DateTime<Utc>,
    pub source_session_id: Option<i64>,
    pub snapshot_id: Option<i64>,
    pub evm: Option<EvmView>,
    pub solana: Option<SolanaView>,
    /// Test/offline market simulation model. Never a live broadcast.
    pub sim_model: Option<crate::security::evm::simulation::TokenSimModel>,
}

impl SecurityContext {
    pub fn from_token(token: TokenDiscovered, quality: QualityStatus, historical: bool) -> Self {
        let as_of_time = token.chain_timestamp.unwrap_or(token.observed_at);
        Self {
            as_of_block: token.block_number,
            as_of_slot: token.slot,
            as_of_time,
            token,
            snapshot: None,
            lifecycle: None,
            data_quality: quality,
            historical,
            source_session_id: None,
            snapshot_id: None,
            evm: None,
            solana: None,
            sim_model: None,
        }
    }
}
