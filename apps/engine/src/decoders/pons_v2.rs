use alloy_primitives::{Address, B256};
use alloy_sol_types::{sol, SolEvent};

use crate::domain::raw_event::normalize_address;
use crate::domain::{
    CanonicalEvent, CanonicalStatus, Chain, GraduationModel, LaunchMechanism, Launchpad,
    LifecycleObserved, LifecycleType, RawEvent, TokenDiscovered, TradeObserved, TradeSide,
};
use crate::error::{EngineError, Result};
use crate::registry::{PONS_ABI_VERSION, PONS_V2_FACTORY};

use super::evm_util::{parse_address, parse_b256, parse_bytes, topic_matches, u256_dec};
use super::Decoder;

sol! {
    event TokenLaunched(
        address indexed token,
        address indexed curve,
        address indexed deployer,
        address pairToken,
        uint256 launchConfigId,
        uint256 graduationThreshold
    );
    event LaunchSwept(address indexed token, uint256 quoteOut, uint256 tokenOut);
    event PoolGraduated(
        address indexed token,
        uint256 positionId,
        uint256 tokenAmount,
        uint256 pairTokenAmount
    );
    event CurveBuy(
        address indexed buyer,
        address indexed recipient,
        uint256 quoteIn,
        uint256 tokensOut,
        uint256 fee,
        uint256 tax
    );
    event CurveSell(
        address indexed seller,
        address indexed recipient,
        uint256 tokensIn,
        uint256 quoteOut,
        uint256 fee,
        uint256 tax
    );
    event SnipeTaxCharged(address indexed buyer, uint256 amount);
    event CurveCompleted(address recipient, uint256 quoteOut, uint256 tokenOut);
}

pub const TOKEN_LAUNCHED_TOPIC0: &str =
    "0x8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607";
pub const LAUNCH_SWEPT_TOPIC0: &str =
    "0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4";
pub const POOL_GRADUATED_TOPIC0: &str =
    "0x0a44ef75df69c534f43cd6c1aa3ef8983065fe5fe79ef9e79f6494e6f258c259";
pub const CURVE_BUY_TOPIC0: &str =
    "0xec36bf571f136799e8dc0b0b8bea4b04d8bd3d43de838aab0d5fc21d4cbfc455";
pub const CURVE_SELL_TOPIC0: &str =
    "0x8113d738abdcb6b38357e9d53a54a7157861a09031b453651f0fe7fe151f59df";
pub const SNIPE_TAX_CHARGED_TOPIC0: &str =
    "0x3bc39a5562b28f5fe8f36cecabfbaa12bb969acf05717994709225fc412a9934";
pub const CURVE_COMPLETED_TOPIC0: &str =
    "0xf8d37a90738ae063b8b8058b66f5880cf3cf7ab0c5d4fa78219696591dfbfb67";

pub fn pons_topic0s() -> Vec<&'static str> {
    vec![
        TOKEN_LAUNCHED_TOPIC0,
        LAUNCH_SWEPT_TOPIC0,
        POOL_GRADUATED_TOPIC0,
        CURVE_BUY_TOPIC0,
        CURVE_SELL_TOPIC0,
        SNIPE_TAX_CHARGED_TOPIC0,
        CURVE_COMPLETED_TOPIC0,
    ]
}

pub struct PonsV2Decoder {
    version: &'static str,
}

impl PonsV2Decoder {
    pub fn pinned() -> Self {
        Self {
            version: PONS_ABI_VERSION,
        }
    }

    pub fn with_version(version: &'static str) -> Self {
        Self { version }
    }
}

impl Decoder for PonsV2Decoder {
    fn name(&self) -> &'static str {
        "pons_v2"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        let Some(log) = raw.as_evm() else {
            return false;
        };
        if log.chain != Chain::Robinhood {
            return false;
        }
        let Some(topic) = log.topics.first() else {
            return false;
        };
        let addr = normalize_address(&log.address);
        if addr == PONS_V2_FACTORY
            && (topic_matches(topic, TOKEN_LAUNCHED_TOPIC0)
                || topic_matches(topic, LAUNCH_SWEPT_TOPIC0)
                || topic_matches(topic, POOL_GRADUATED_TOPIC0))
        {
            return true;
        }
        topic_matches(topic, CURVE_BUY_TOPIC0)
            || topic_matches(topic, CURVE_SELL_TOPIC0)
            || topic_matches(topic, SNIPE_TAX_CHARGED_TOPIC0)
            || topic_matches(topic, CURVE_COMPLETED_TOPIC0)
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        if self.version != PONS_ABI_VERSION {
            return Err(EngineError::DecoderVersionMismatch {
                protocol: self.name().to_string(),
                requested: self.version.to_string(),
                pinned: PONS_ABI_VERSION.to_string(),
            });
        }
        let log = raw
            .as_evm()
            .ok_or_else(|| EngineError::DecoderMismatch("pons decoder requires evm log".into()))?;
        let topic = log
            .topics
            .first()
            .map(|t| normalize_address(t))
            .unwrap_or_default();
        if topic == TOKEN_LAUNCHED_TOPIC0 {
            return decode_token_launched(self, raw);
        }
        if topic == LAUNCH_SWEPT_TOPIC0 {
            return decode_launch_swept(self, raw);
        }
        if topic == POOL_GRADUATED_TOPIC0 {
            return decode_pool_graduated(self, raw);
        }
        if topic == CURVE_BUY_TOPIC0 {
            return decode_curve_trade(self, raw, TradeSide::Buy);
        }
        if topic == CURVE_SELL_TOPIC0 {
            return decode_curve_trade(self, raw, TradeSide::Sell);
        }
        if topic == SNIPE_TAX_CHARGED_TOPIC0 {
            return decode_snipe_tax(self, raw);
        }
        if topic == CURVE_COMPLETED_TOPIC0 {
            return decode_curve_completed(self, raw);
        }
        Ok(Vec::new())
    }
}

fn primitive_log(raw: &RawEvent) -> Result<(alloy_primitives::Log, crate::domain::EvmLog)> {
    let log = raw
        .as_evm()
        .ok_or_else(|| EngineError::DecoderMismatch("evm".into()))?
        .clone();
    let address = parse_address(&log.address)?;
    let topics: Vec<B256> = log
        .topics
        .iter()
        .map(|t| parse_b256(t))
        .collect::<Result<Vec<_>>>()?;
    let data = parse_bytes(&log.data)?;
    let primitive = alloy_primitives::Log::new(address, topics, data)
        .ok_or_else(|| EngineError::Malformed("pons log could not be constructed".into()))?;
    Ok((primitive, log))
}

fn decode_token_launched(decoder: &PonsV2Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    if log.topics.len() < 4 {
        return Err(EngineError::Malformed(
            "pons TokenLaunched missing indexed topics".into(),
        ));
    }
    let decoded = TokenLaunched::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("pons TokenLaunched decode: {e}")))?;
    let token_address = normalize_address(&decoded.token.to_string());
    let curve = normalize_address(&decoded.curve.to_string());
    let token = TokenDiscovered {
        chain: Chain::Robinhood,
        chain_id: Some(log.chain_id),
        token_address: token_address.clone(),
        creator: normalize_address(&decoded.deployer.to_string()),
        launchpad: Launchpad::PonsV2,
        factory_or_program: PONS_V2_FACTORY.to_string(),
        pool: None,
        curve: Some(curve.clone()),
        quote_asset: Some(normalize_address(&decoded.pairToken.to_string())),
        launch_mechanism: LaunchMechanism::BondingCurve,
        bonding_curve: true,
        graduation_model: GraduationModel::PonsV4Hook,
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
        decoder_version: decoder.version.to_string(),
        initial_liquidity: None,
        raw_event_id: raw.event_id(),
    };
    let life = evm_lifecycle(
        decoder,
        raw,
        &log,
        &token_address,
        LifecycleType::TokenCreated,
        Some(PONS_V2_FACTORY.to_string()),
        None,
        Some(curve),
        serde_json::json!({
            "launchConfigId": u256_dec(decoded.launchConfigId),
            "graduationThreshold": u256_dec(decoded.graduationThreshold),
            "pairToken": normalize_address(&decoded.pairToken.to_string()),
        }),
    );
    Ok(vec![
        CanonicalEvent::TokenDiscovered(Box::new(token)),
        CanonicalEvent::Lifecycle(Box::new(life)),
    ])
}

fn decode_launch_swept(decoder: &PonsV2Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    let decoded = LaunchSwept::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("pons LaunchSwept decode: {e}")))?;
    let token = normalize_address(&decoded.token.to_string());
    let life = evm_lifecycle(
        decoder,
        raw,
        &log,
        &token,
        LifecycleType::LaunchSwept,
        Some(PONS_V2_FACTORY.to_string()),
        None,
        None,
        serde_json::json!({
            "quoteOut": u256_dec(decoded.quoteOut),
            "tokenOut": u256_dec(decoded.tokenOut),
        }),
    );
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn decode_pool_graduated(decoder: &PonsV2Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    let decoded = PoolGraduated::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("pons PoolGraduated decode: {e}")))?;
    let token = normalize_address(&decoded.token.to_string());
    let life = evm_lifecycle(
        decoder,
        raw,
        &log,
        &token,
        LifecycleType::PoolGraduated,
        Some(PONS_V2_FACTORY.to_string()),
        None,
        None,
        serde_json::json!({
            "positionId": u256_dec(decoded.positionId),
            "tokenAmount": u256_dec(decoded.tokenAmount),
            "pairTokenAmount": u256_dec(decoded.pairTokenAmount),
        }),
    );
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn decode_curve_trade(
    decoder: &PonsV2Decoder,
    raw: &RawEvent,
    side: TradeSide,
) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    let curve = normalize_address(&log.address);
    let (trader, base, quote, meta) = match side {
        TradeSide::Buy => {
            let d = CurveBuy::decode_log(&primitive)
                .map_err(|e| EngineError::Malformed(format!("pons CurveBuy decode: {e}")))?;
            (
                normalize_address(&d.buyer.to_string()),
                u256_dec(d.tokensOut),
                u256_dec(d.quoteIn),
                serde_json::json!({
                    "recipient": normalize_address(&d.recipient.to_string()),
                    "fee": u256_dec(d.fee),
                    "tax": u256_dec(d.tax),
                }),
            )
        }
        TradeSide::Sell => {
            let d = CurveSell::decode_log(&primitive)
                .map_err(|e| EngineError::Malformed(format!("pons CurveSell decode: {e}")))?;
            (
                normalize_address(&d.seller.to_string()),
                u256_dec(d.tokensIn),
                u256_dec(d.quoteOut),
                serde_json::json!({
                    "recipient": normalize_address(&d.recipient.to_string()),
                    "fee": u256_dec(d.fee),
                    "tax": u256_dec(d.tax),
                }),
            )
        }
    };
    let trade = TradeObserved {
        event_id: raw.event_id(),
        chain: Chain::Robinhood,
        launchpad: Launchpad::PonsV2,
        token_address: String::new(),
        trader,
        side,
        base_amount_raw: base,
        quote_amount_raw: quote,
        base_decimals: 18,
        quote_decimals: 18,
        quote_asset: "0x0000000000000000000000000000000000000000".into(),
        pool: None,
        curve: Some(curve),
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
        metadata: meta,
    };
    Ok(vec![CanonicalEvent::Trade(Box::new(trade))])
}

fn decode_snipe_tax(decoder: &PonsV2Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    let decoded = SnipeTaxCharged::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("pons SnipeTaxCharged decode: {e}")))?;
    let curve = normalize_address(&log.address);
    let life = evm_lifecycle(
        decoder,
        raw,
        &log,
        "",
        LifecycleType::SnipeTaxCharged,
        None,
        None,
        Some(curve),
        serde_json::json!({
            "buyer": normalize_address(&decoded.buyer.to_string()),
            "amount": u256_dec(decoded.amount),
        }),
    );
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn decode_curve_completed(decoder: &PonsV2Decoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let (primitive, log) = primitive_log(raw)?;
    let decoded = CurveCompleted::decode_log(&primitive)
        .map_err(|e| EngineError::Malformed(format!("pons CurveCompleted decode: {e}")))?;
    let curve = normalize_address(&log.address);
    let life = evm_lifecycle(
        decoder,
        raw,
        &log,
        "",
        LifecycleType::CurveCompleted,
        None,
        None,
        Some(curve),
        serde_json::json!({
            "recipient": normalize_address(&decoded.recipient.to_string()),
            "quoteOut": u256_dec(decoded.quoteOut),
            "tokenOut": u256_dec(decoded.tokenOut),
        }),
    );
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

#[allow(clippy::too_many_arguments)]
fn evm_lifecycle(
    decoder: &PonsV2Decoder,
    raw: &RawEvent,
    log: &crate::domain::EvmLog,
    token: &str,
    kind: LifecycleType,
    factory: Option<String>,
    pool: Option<String>,
    curve: Option<String>,
    metadata: serde_json::Value,
) -> LifecycleObserved {
    LifecycleObserved {
        event_id: raw.event_id(),
        chain: Chain::Robinhood,
        launchpad: Launchpad::PonsV2,
        token_address: token.to_string(),
        lifecycle_type: kind,
        factory,
        pool,
        curve,
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
        metadata,
    }
}

pub fn _keep_address_type(a: Address) -> Address {
    a
}
