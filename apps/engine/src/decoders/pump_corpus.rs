//! Adapter: decoded Pump.fun research corpus → canonical events.
//! Does not pretend tabular rows are raw chain instructions.

use crate::domain::{
    classify_amount, CanonicalEvent, CanonicalStatus, Chain, CorpusEventType, GraduationModel,
    IdentityQuality, LaunchMechanism, Launchpad, LifecycleObserved, LifecycleType, RawEvent,
    TokenDiscovered, TradeObserved, NORMALIZATION_VERSION,
};
use crate::error::Result;
use crate::registry::{PUMPFUN_PROGRAM, SOL_MINT};

use super::Decoder;

pub struct PumpCorpusDecoder {
    version: &'static str,
}

impl PumpCorpusDecoder {
    pub fn pinned() -> Self {
        Self {
            version: NORMALIZATION_VERSION,
        }
    }
}

impl Decoder for PumpCorpusDecoder {
    fn name(&self) -> &'static str {
        "pumpfun_corpus"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        raw.as_corpus().is_some()
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        let Some(c) = raw.as_corpus() else {
            return Ok(vec![]);
        };
        match c.event_type {
            CorpusEventType::Launch => Ok(vec![CanonicalEvent::TokenDiscovered(Box::new(
                TokenDiscovered {
                    chain: Chain::Solana,
                    chain_id: None,
                    token_address: c.mint.clone(),
                    creator: c.creator.clone().unwrap_or_default(),
                    launchpad: Launchpad::PumpFun,
                    factory_or_program: PUMPFUN_PROGRAM.into(),
                    pool: None,
                    curve: None,
                    quote_asset: Some(SOL_MINT.into()),
                    launch_mechanism: LaunchMechanism::BondingCurve,
                    bonding_curve: true,
                    graduation_model: GraduationModel::PumpAmm,
                    block_number: None,
                    block_hash: None,
                    slot: c.slot,
                    tx_hash_or_signature: c.derived_tx_id(),
                    instruction_index: c.instruction_index,
                    inner_instruction_index: c.inner_instruction_index,
                    log_index: Some(c.source_row),
                    chain_timestamp: Some(c.timestamp),
                    observed_at: raw.observed_at,
                    persisted_at: None,
                    source: raw.source.clone(),
                    decoder_version: self.version.to_string(),
                    initial_liquidity: None,
                    raw_event_id: raw.event_id(),
                },
            ))]),
            CorpusEventType::Trade => {
                let (amt_q, token_int) = classify_amount(c.token_amount.as_deref());
                let (_q2, sol_int) = classify_amount(c.sol_amount.as_deref());
                let usable = amt_q.execution_usable();
                let base = if usable {
                    token_int.clone().unwrap_or_else(|| "0".into())
                } else {
                    "0".into()
                };
                let quote = if usable {
                    sol_int.clone().unwrap_or_else(|| "0".into())
                } else {
                    "0".into()
                };
                Ok(vec![CanonicalEvent::Trade(Box::new(TradeObserved {
                    event_id: raw.event_id(),
                    chain: Chain::Solana,
                    launchpad: Launchpad::PumpFun,
                    token_address: c.mint.clone(),
                    trader: c.trader.clone().unwrap_or_default(),
                    side: c.side.unwrap_or(crate::domain::TradeSide::Buy),
                    base_amount_raw: base,
                    quote_amount_raw: quote,
                    base_decimals: 6,
                    quote_decimals: 9,
                    quote_asset: SOL_MINT.into(),
                    pool: None,
                    curve: None,
                    price_estimate: None,
                    block_number: None,
                    block_hash: None,
                    slot: c.slot,
                    transaction_index: c.transaction_index.or(Some(c.order_seq)),
                    tx_hash_or_signature: c.derived_tx_id(),
                    log_index: Some(c.source_row),
                    instruction_index: c.instruction_index,
                    inner_instruction_index: c.inner_instruction_index,
                    chain_timestamp: Some(c.timestamp),
                    observed_at: raw.observed_at,
                    persisted_at: None,
                    canonical_status: CanonicalStatus::Canonical,
                    finality: raw.finality,
                    source: raw.source.clone(),
                    decoder_version: self.version.to_string(),
                    raw_event_id: raw.event_id(),
                    metadata: serde_json::json!({
                        "source_kind": c.source_kind.as_str(),
                        "source_dataset_id": c.dataset_id,
                        "source_file": c.source_file,
                        "source_row": c.source_row,
                        "identity_quality": c.identity_quality.as_str(),
                        "amount_quality": c.amount_quality.as_str(),
                        "data_quality": c.data_quality,
                        "normalization_version": c.normalization_version,
                        "original_timestamp": c.timestamp,
                        "original_token_amount": c.token_amount,
                        "original_sol_amount": c.sol_amount,
                        "v_sol_bonding_curve": c.v_sol_bonding_curve,
                        "v_tokens_bonding_curve": c.v_tokens_bonding_curve,
                        "integer_fill_usable": usable,
                    }),
                }))])
            }
            CorpusEventType::Graduation => {
                let pool = c
                    .original
                    .get("pool_address")
                    .and_then(|v| v.as_str())
                    .filter(|s| {
                        *s != "synthetic_graduation_queue" && *s != "backfilled_from_pumpswap_trade"
                    })
                    .map(|s| s.to_string());
                Ok(vec![CanonicalEvent::Lifecycle(Box::new(
                    LifecycleObserved {
                        event_id: raw.event_id(),
                        chain: Chain::Solana,
                        launchpad: Launchpad::PumpFun,
                        token_address: c.mint.clone(),
                        lifecycle_type: LifecycleType::Migrated,
                        factory: Some(PUMPFUN_PROGRAM.into()),
                        pool,
                        curve: None,
                        block_number: None,
                        block_hash: None,
                        slot: c.slot,
                        transaction_index: c.transaction_index.or(Some(c.order_seq)),
                        tx_hash_or_signature: c.derived_tx_id(),
                        log_index: Some(c.source_row),
                        instruction_index: c.instruction_index,
                        inner_instruction_index: c.inner_instruction_index,
                        chain_timestamp: Some(c.timestamp),
                        observed_at: raw.observed_at,
                        persisted_at: None,
                        canonical_status: CanonicalStatus::Canonical,
                        finality: raw.finality,
                        source: raw.source.clone(),
                        decoder_version: self.version.to_string(),
                        raw_event_id: raw.event_id(),
                        metadata: serde_json::json!({
                            "source_kind": c.source_kind.as_str(),
                            "identity_quality": IdentityQuality::Derived.as_str(),
                            "source_file": c.source_file,
                            "source_row": c.source_row,
                            "normalization_version": c.normalization_version,
                        }),
                    },
                ))])
            }
        }
    }
}
