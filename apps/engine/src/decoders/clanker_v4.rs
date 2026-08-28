use alloy_primitives::{Address, B256};
use alloy_sol_types::{sol, SolEvent};

use crate::domain::raw_event::normalize_address;
use crate::domain::{
    CanonicalEvent, CanonicalStatus, Chain, GraduationModel, LaunchMechanism, Launchpad,
    LifecycleObserved, LifecycleType, RawEvent, TokenDiscovered,
};
use crate::error::{EngineError, Result};
use crate::registry::{CLANKER_ABI_VERSION, CLANKER_V4_FACTORY};

use super::evm_util::{parse_address, parse_b256, parse_bytes};
use super::Decoder;

sol! {
    event TokenCreated(
        address msgSender,
        address indexed tokenAddress,
        address indexed tokenAdmin,
        string tokenImage,
        string tokenName,
        string tokenSymbol,
        string tokenMetadata,
        string tokenContext,
        int24 startingTick,
        address poolHook,
        bytes32 poolId,
        address pairedToken,
        address locker,
        address mevModule,
        uint256 extensionsSupply,
        address[] extensions
    );
}

pub const TOKEN_CREATED_TOPIC0: &str =
    "0x9299d1d1a88d8e1abdc591ae7a167a6bc63a8f17d695804e9091ee33aa89fb67";

pub struct ClankerV4Decoder {
    version: &'static str,
}

impl ClankerV4Decoder {
    pub fn pinned() -> Self {
        Self {
            version: CLANKER_ABI_VERSION,
        }
    }

    pub fn with_version(version: &'static str) -> Self {
        Self { version }
    }
}

impl Decoder for ClankerV4Decoder {
    fn name(&self) -> &'static str {
        "clanker_v4"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        let Some(log) = raw.as_evm() else {
            return false;
        };
        if log.chain != Chain::Base {
            return false;
        }
        if normalize_address(&log.address) != CLANKER_V4_FACTORY {
            return false;
        }
        log.topics
            .first()
            .map(|t| normalize_address(t) == TOKEN_CREATED_TOPIC0)
            .unwrap_or(false)
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        if self.version != CLANKER_ABI_VERSION {
            return Err(EngineError::DecoderVersionMismatch {
                protocol: self.name().to_string(),
                requested: self.version.to_string(),
                pinned: CLANKER_ABI_VERSION.to_string(),
            });
        }
        let log = raw.as_evm().ok_or_else(|| {
            EngineError::DecoderMismatch("clanker decoder requires evm log".into())
        })?;
        if log.topics.len() < 3 {
            return Err(EngineError::Malformed(
                "clanker TokenCreated missing indexed topics".into(),
            ));
        }
        let address = parse_address(&log.address)?;
        let topics: Vec<B256> = log
            .topics
            .iter()
            .map(|t| parse_b256(t))
            .collect::<Result<Vec<_>>>()?;
        let data = parse_bytes(&log.data)?;
        let primitive = alloy_primitives::Log::new(address, topics, data)
            .ok_or_else(|| EngineError::Malformed("clanker log could not be constructed".into()))?;
        let decoded = TokenCreated::decode_log(&primitive)
            .map_err(|e| EngineError::Malformed(format!("clanker TokenCreated decode: {e}")))?;

        let creator = normalize_address(&decoded.msgSender.to_string());
        let token_address = normalize_address(&decoded.tokenAddress.to_string());
        let pool = format!("{:#x}", decoded.poolId);
        let hook = normalize_address(&decoded.poolHook.to_string());
        let quote = normalize_address(&decoded.pairedToken.to_string());
        let token = TokenDiscovered {
            chain: Chain::Base,
            chain_id: Some(log.chain_id),
            token_address: token_address.clone(),
            creator,
            launchpad: Launchpad::ClankerV4,
            factory_or_program: CLANKER_V4_FACTORY.to_string(),
            pool: Some(pool.clone()),
            curve: None,
            quote_asset: Some(quote.clone()),
            launch_mechanism: LaunchMechanism::LockedV4,
            bonding_curve: false,
            graduation_model: GraduationModel::None,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            slot: None,
            tx_hash_or_signature: normalize_address(&log.transaction_hash),
            instruction_index: None,
            inner_instruction_index: None,
            log_index: Some(log.log_index),
            chain_timestamp: log.block_timestamp,
            observed_at: raw.observed_at,
            persisted_at: None,
            source: raw.source.clone(),
            decoder_version: self.version.to_string(),
            initial_liquidity: None,
            raw_event_id: raw.event_id(),
        };
        let created = LifecycleObserved {
            event_id: raw.event_id(),
            chain: Chain::Base,
            launchpad: Launchpad::ClankerV4,
            token_address: token_address.clone(),
            lifecycle_type: LifecycleType::TokenCreated,
            factory: Some(CLANKER_V4_FACTORY.to_string()),
            pool: Some(pool.clone()),
            curve: None,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            slot: None,
            transaction_index: log.transaction_index,
            tx_hash_or_signature: normalize_address(&log.transaction_hash),
            log_index: Some(log.log_index),
            instruction_index: None,
            inner_instruction_index: None,
            chain_timestamp: log.block_timestamp,
            observed_at: raw.observed_at,
            persisted_at: None,
            canonical_status: CanonicalStatus::Canonical,
            finality: raw.finality,
            source: raw.source.clone(),
            decoder_version: self.version.to_string(),
            raw_event_id: raw.event_id(),
            metadata: serde_json::json!({
                "tokenAdmin": normalize_address(&decoded.tokenAdmin.to_string()),
                "poolHook": hook,
                "locker": normalize_address(&decoded.locker.to_string()),
                "pairedToken": quote,
                "tokenName": decoded.tokenName,
                "tokenSymbol": decoded.tokenSymbol,
            }),
        };
        Ok(vec![
            CanonicalEvent::TokenDiscovered(Box::new(token)),
            CanonicalEvent::Lifecycle(Box::new(created)),
        ])
    }
}

pub fn _keep_address(a: Address) -> Address {
    a
}
