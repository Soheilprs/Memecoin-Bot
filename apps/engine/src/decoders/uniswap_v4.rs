use alloy_primitives::{Address, B256};
use alloy_sol_types::{sol, SolEvent};

use crate::domain::raw_event::normalize_address;
use crate::domain::{
    CanonicalEvent, CanonicalStatus, Chain, Launchpad, LifecycleObserved, LifecycleType, RawEvent,
    TradeObserved, TradeSide,
};
use crate::error::{EngineError, Result};
use crate::registry::{
    BASE_V4_POOL_MANAGER, CLANKER_ABI_VERSION, ROBINHOOD_V4_POOL_MANAGER, UNISWAP_V4_ABI_VERSION,
    WETH_BASE,
};

use super::evm_util::{i128_abs_dec, parse_address, parse_b256, parse_bytes, topic_matches};
use super::Decoder;

sol! {
    event Initialize(
        bytes32 indexed id,
        address indexed currency0,
        address indexed currency1,
        uint24 fee,
        int24 tickSpacing,
        address hooks,
        uint160 sqrtPriceX96,
        int24 tick
    );
    event Swap(
        bytes32 indexed id,
        address indexed sender,
        int128 amount0,
        int128 amount1,
        uint160 sqrtPriceX96,
        uint128 liquidity,
        int24 tick,
        uint24 fee
    );
}

pub const SWAP_TOPIC0: &str = "0x40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f";
pub const INITIALIZE_TOPIC0: &str =
    "0xdd466e674ea557f56295e2d0218a125ea4b4f0f6f3307b95f85e6110838d6438";

pub fn is_pool_manager(chain: Chain, address: &str) -> bool {
    let addr = normalize_address(address);
    match chain {
        Chain::Base => addr == BASE_V4_POOL_MANAGER,
        Chain::Robinhood => addr == ROBINHOOD_V4_POOL_MANAGER,
        Chain::Solana => false,
    }
}

pub struct UniswapV4Decoder {
    version: &'static str,
}

impl UniswapV4Decoder {
    pub fn pinned() -> Self {
        Self {
            version: UNISWAP_V4_ABI_VERSION,
        }
    }
}

impl Decoder for UniswapV4Decoder {
    fn name(&self) -> &'static str {
        "uniswap_v4"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        let Some(log) = raw.as_evm() else {
            return false;
        };
        if !is_pool_manager(log.chain, &log.address) {
            return false;
        }
        log.topics
            .first()
            .map(|t| topic_matches(t, SWAP_TOPIC0) || topic_matches(t, INITIALIZE_TOPIC0))
            .unwrap_or(false)
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        let log = raw.as_evm().ok_or_else(|| {
            EngineError::DecoderMismatch("uniswap v4 decoder requires evm log".into())
        })?;
        let topic = log
            .topics
            .first()
            .map(|t| normalize_address(t))
            .unwrap_or_default();
        if topic == INITIALIZE_TOPIC0 {
            return decode_initialize(self, raw);
        }
        if topic == SWAP_TOPIC0 {
            return decode_swap(self, raw);
        }
        Ok(Vec::new())
    }
}

fn primitive(raw: &RawEvent) -> Result<(alloy_primitives::Log, crate::domain::EvmLog)> {
    let log = raw.as_evm().unwrap().clone();
    let address = parse_address(&log.address)?;
    let topics: Vec<B256> = log
        .topics
        .iter()
        .map(|t| parse_b256(t))
        .collect::<Result<Vec<_>>>()?;
    let data = parse_bytes(&log.data)?;
    let primitive = alloy_primitives::Log::new(address, topics, data)
        .ok_or_else(|| EngineError::Malformed("v4 log could not be constructed".into()))?;
    Ok((primitive, log))
}

fn launchpad_for(chain: Chain) -> Launchpad {
    match chain {
        Chain::Base => Launchpad::ClankerV4,
        Chain::Robinhood => Launchpad::PonsV2,
        Chain::Solana => Launchpad::Unknown,
    }
}

fn decode_initialize(decoder: &UniswapV4Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive(raw)?;
    let decoded = Initialize::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("v4 Initialize decode: {e}")))?;
    let pool = format!("{:#x}", decoded.id);
    let hooks = normalize_address(&decoded.hooks.to_string());
    let c0 = normalize_address(&decoded.currency0.to_string());
    let c1 = normalize_address(&decoded.currency1.to_string());
    let life = LifecycleObserved {
        event_id: raw.event_id(),
        chain: log.chain,
        launchpad: launchpad_for(log.chain),
        token_address: String::new(),
        lifecycle_type: LifecycleType::PoolCreated,
        factory: Some(normalize_address(&log.address)),
        pool: Some(pool),
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
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata: serde_json::json!({
            "currency0": c0,
            "currency1": c1,
            "hooks": hooks,
            "fee": decoded.fee.to_string(),
            "tickSpacing": decoded.tickSpacing.to_string(),
            "sqrtPriceX96": decoded.sqrtPriceX96.to_string(),
            "tick": decoded.tick.to_string(),
        }),
    };
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn decode_swap(decoder: &UniswapV4Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive(raw)?;
    let decoded = Swap::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("v4 Swap decode: {e}")))?;
    let pool = format!("{:#x}", decoded.id);
    let trader = normalize_address(&decoded.sender.to_string());
    let (side, base, quote) = swap_side_and_amounts(decoded.amount0, decoded.amount1);
    let trade = TradeObserved {
        event_id: raw.event_id(),
        chain: log.chain,
        launchpad: launchpad_for(log.chain),
        token_address: String::new(),
        trader,
        side,
        base_amount_raw: base,
        quote_amount_raw: quote,
        base_decimals: 18,
        quote_decimals: 18,
        quote_asset: if log.chain == Chain::Base {
            WETH_BASE.to_string()
        } else {
            "0x0000000000000000000000000000000000000000".into()
        },
        pool: Some(pool),
        curve: None,
        price_estimate: None,
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
        canonical_status: if log.removed {
            CanonicalStatus::Orphaned
        } else {
            CanonicalStatus::Canonical
        },
        finality: raw.finality,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata: serde_json::json!({
            "amount0": decoded.amount0.to_string(),
            "amount1": decoded.amount1.to_string(),
            "sqrtPriceX96": decoded.sqrtPriceX96.to_string(),
            "liquidity": decoded.liquidity.to_string(),
            "tick": decoded.tick.to_string(),
            "fee": decoded.fee.to_string(),
            "side_assumption": "currency0_is_quote_when_weth_or_native",
            "decoder_note": CLANKER_ABI_VERSION,
        }),
    };
    Ok(vec![CanonicalEvent::Trade(Box::new(trade))])
}

/// Uniswap v4: positive amount = into the pool. If currency0 is the quote
/// (WETH on Base, native on many Pons pools), amount1 < 0 means the token
/// leaves the pool (user buys).
pub fn swap_side_and_amounts(amount0: i128, amount1: i128) -> (TradeSide, String, String) {
    let side = if amount1 < 0 {
        TradeSide::Buy
    } else {
        TradeSide::Sell
    };
    (side, i128_abs_dec(amount1), i128_abs_dec(amount0))
}

pub fn _keep_address(a: Address) -> Address {
    a
}
