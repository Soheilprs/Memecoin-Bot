//! PumpSwap (Pump AMM) decoder for migrated Pump.fun tokens.

use base64::Engine as _;

use crate::domain::{
    CanonicalEvent, Chain, Launchpad, LifecycleObserved, LifecycleType, RawEvent, TradeObserved,
    TradeSide,
};
use crate::error::{EngineError, Result};
use crate::registry::{PUMPFUN_IDL_VERSION, PUMPSWAP_PROGRAM, SOL_MINT};

use super::solana_buf::{decode_ix_data, disc_eq, read_i64, read_pubkey, read_u16, read_u64};
use super::Decoder;

pub const CREATE_POOL_DISCRIMINATOR: [u8; 8] = [233, 146, 209, 142, 207, 104, 64, 188];
pub const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
pub const BUY_EXACT_QUOTE_IN_DISCRIMINATOR: [u8; 8] = [198, 46, 21, 82, 180, 217, 232, 112];
pub const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

pub const CREATE_POOL_EVENT_DISCRIMINATOR: [u8; 8] = [177, 49, 12, 210, 160, 118, 167, 116];
pub const BUY_EVENT_DISCRIMINATOR: [u8; 8] = [103, 244, 82, 31, 44, 245, 119, 119];
pub const SELL_EVENT_DISCRIMINATOR: [u8; 8] = [62, 47, 55, 10, 165, 3, 220, 42];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpSwapIxKind {
    CreatePool,
    Buy,
    Sell,
}

pub fn classify_pumpswap_ix(data: &[u8]) -> Option<PumpSwapIxKind> {
    if data.len() < 8 {
        return None;
    }
    let d: [u8; 8] = data[..8].try_into().ok()?;
    Some(match d {
        CREATE_POOL_DISCRIMINATOR => PumpSwapIxKind::CreatePool,
        BUY_DISCRIMINATOR | BUY_EXACT_QUOTE_IN_DISCRIMINATOR => PumpSwapIxKind::Buy,
        SELL_DISCRIMINATOR => PumpSwapIxKind::Sell,
        _ => return None,
    })
}

pub struct PumpSwapDecoder {
    version: &'static str,
}

impl PumpSwapDecoder {
    pub fn pinned() -> Self {
        Self {
            version: PUMPFUN_IDL_VERSION,
        }
    }
}

impl Decoder for PumpSwapDecoder {
    fn name(&self) -> &'static str {
        "pumpswap"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        let Some(ix) = raw.as_solana() else {
            return false;
        };
        if ix.program_id != PUMPSWAP_PROGRAM {
            return false;
        }
        match decode_ix_data(&ix.data_base58) {
            Ok(bytes) => classify_pumpswap_ix(&bytes).is_some(),
            _ => false,
        }
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        let ix = raw.as_solana().ok_or_else(|| {
            EngineError::DecoderMismatch("pumpswap decoder requires solana instruction".into())
        })?;
        if ix.execution_status == crate::domain::ExecutionStatus::Failed {
            return Ok(Vec::new());
        }
        let data = decode_ix_data(&ix.data_base58)?;
        let kind = classify_pumpswap_ix(&data).ok_or_else(|| {
            EngineError::DecoderMismatch("not a tracked pumpswap instruction".into())
        })?;
        match kind {
            PumpSwapIxKind::CreatePool => decode_create_pool(self, raw),
            PumpSwapIxKind::Buy | PumpSwapIxKind::Sell => decode_swap(self, raw, kind),
        }
    }
}

fn decode_create_pool(decoder: &PumpSwapDecoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let ix = raw.as_solana().unwrap();
    let event = extract_create_pool_event(&ix.log_messages).ok();
    let (mint, pool, quote, extra) = if let Some(e) = event {
        (
            e.base_mint,
            Some(e.pool.clone()),
            Some(e.quote_mint),
            serde_json::json!({
                "instruction": "create_pool",
                "creator": e.creator,
                "base_amount_in": e.base_amount_in.to_string(),
                "quote_amount_in": e.quote_amount_in.to_string(),
                "initial_liquidity": e.initial_liquidity.to_string(),
                "lp_mint": e.lp_mint,
            }),
        )
    } else {
        (
            ix.accounts.get(3).cloned().unwrap_or_default(),
            ix.accounts.first().cloned(),
            ix.accounts.get(4).cloned(),
            serde_json::json!({ "instruction": "create_pool" }),
        )
    };
    let life = LifecycleObserved {
        event_id: raw.event_id(),
        chain: Chain::Solana,
        launchpad: Launchpad::PumpSwap,
        token_address: mint,
        lifecycle_type: LifecycleType::PoolCreated,
        factory: Some(PUMPSWAP_PROGRAM.to_string()),
        pool,
        curve: None,
        block_number: None,
        block_hash: None,
        slot: ix.slot,
        transaction_index: ix.transaction_index.map(|v| v as u64),
        tx_hash_or_signature: ix.signature.clone(),
        log_index: None,
        instruction_index: Some(ix.instruction_index),
        inner_instruction_index: ix.inner_instruction_index,
        chain_timestamp: ix.block_time,
        observed_at: raw.observed_at,
        persisted_at: None,
        canonical_status: raw.canonical_status,
        finality: raw.finality,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata: extra,
    };
    let _ = quote;
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn decode_swap(
    decoder: &PumpSwapDecoder,
    raw: &RawEvent,
    kind: PumpSwapIxKind,
) -> Result<Vec<CanonicalEvent>> {
    let ix = raw.as_solana().unwrap();
    let (side, base, quote, trader, pool) = match kind {
        PumpSwapIxKind::Buy => {
            let e = extract_buy_event(&ix.log_messages)?;
            (
                TradeSide::Buy,
                e.base_amount_out,
                e.user_quote_amount_in,
                e.user,
                e.pool,
            )
        }
        PumpSwapIxKind::Sell => {
            let e = extract_sell_event(&ix.log_messages)?;
            (
                TradeSide::Sell,
                e.base_amount_in,
                e.user_quote_amount_out,
                e.user,
                e.pool,
            )
        }
        PumpSwapIxKind::CreatePool => unreachable!(),
    };
    let mint = ix.accounts.get(3).cloned().unwrap_or_default();
    let trade = TradeObserved {
        event_id: raw.event_id(),
        chain: Chain::Solana,
        launchpad: Launchpad::PumpSwap,
        token_address: mint,
        trader,
        side,
        base_amount_raw: base.to_string(),
        quote_amount_raw: quote.to_string(),
        base_decimals: 6,
        quote_decimals: 9,
        quote_asset: SOL_MINT.to_string(),
        pool: Some(pool),
        curve: None,
        price_estimate: None,
        block_number: None,
        block_hash: None,
        slot: ix.slot,
        transaction_index: ix.transaction_index.map(|v| v as u64),
        tx_hash_or_signature: ix.signature.clone(),
        log_index: None,
        instruction_index: Some(ix.instruction_index),
        inner_instruction_index: ix.inner_instruction_index,
        chain_timestamp: ix.block_time,
        observed_at: raw.observed_at,
        persisted_at: None,
        canonical_status: raw.canonical_status,
        finality: raw.finality,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata: serde_json::json!({
            "instruction": match kind {
                PumpSwapIxKind::Buy => "buy",
                PumpSwapIxKind::Sell => "sell",
                PumpSwapIxKind::CreatePool => "create_pool",
            },
            "token_balances": ix.token_balances,
        }),
    };
    Ok(vec![CanonicalEvent::Trade(Box::new(trade))])
}

struct CreatePoolEv {
    creator: String,
    base_mint: String,
    quote_mint: String,
    base_amount_in: u64,
    quote_amount_in: u64,
    initial_liquidity: u64,
    pool: String,
    lp_mint: String,
}

struct BuyEv {
    base_amount_out: u64,
    user_quote_amount_in: u64,
    pool: String,
    user: String,
}

struct SellEv {
    base_amount_in: u64,
    user_quote_amount_out: u64,
    pool: String,
    user: String,
}

fn program_data(line: &str) -> Option<Vec<u8>> {
    let b64 = line.strip_prefix("Program data: ")?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()
}

fn extract_create_pool_event(logs: &[String]) -> Result<CreatePoolEv> {
    for line in logs {
        let Some(bytes) = program_data(line) else {
            continue;
        };
        if !disc_eq(&bytes, &CREATE_POOL_EVENT_DISCRIMINATOR) {
            continue;
        }
        let mut i = 0usize;
        let buf = &bytes[8..];
        let _ts = read_i64(buf, &mut i)?;
        let _index = read_u16(buf, &mut i)?;
        let creator = read_pubkey(buf, &mut i)?;
        let base_mint = read_pubkey(buf, &mut i)?;
        let quote_mint = read_pubkey(buf, &mut i)?;
        if i >= buf.len() {
            return Err(EngineError::Malformed("create pool event short".into()));
        }
        i += 2; // two u8 decimals
        let base_amount_in = read_u64(buf, &mut i)?;
        let quote_amount_in = read_u64(buf, &mut i)?;
        let _pool_base = read_u64(buf, &mut i)?;
        let _pool_quote = read_u64(buf, &mut i)?;
        let _min_liq = read_u64(buf, &mut i)?;
        let initial_liquidity = read_u64(buf, &mut i)?;
        let _lp_out = read_u64(buf, &mut i)?;
        if i >= buf.len() {
            return Err(EngineError::Malformed("create pool bump".into()));
        }
        i += 1;
        let pool = read_pubkey(buf, &mut i)?;
        let lp_mint = read_pubkey(buf, &mut i)?;
        return Ok(CreatePoolEv {
            creator,
            base_mint,
            quote_mint,
            base_amount_in,
            quote_amount_in,
            initial_liquidity,
            pool,
            lp_mint,
        });
    }
    Err(EngineError::Malformed("CreatePoolEvent not found".into()))
}

fn extract_buy_event(logs: &[String]) -> Result<BuyEv> {
    for line in logs {
        let Some(bytes) = program_data(line) else {
            continue;
        };
        if !disc_eq(&bytes, &BUY_EVENT_DISCRIMINATOR) {
            continue;
        }
        let mut i = 0usize;
        let buf = &bytes[8..];
        let _ts = read_i64(buf, &mut i)?;
        let base_amount_out = read_u64(buf, &mut i)?;
        let _max = read_u64(buf, &mut i)?;
        let _ubr = read_u64(buf, &mut i)?;
        let _uqr = read_u64(buf, &mut i)?;
        let _pbr = read_u64(buf, &mut i)?;
        let _pqr = read_u64(buf, &mut i)?;
        let _qin = read_u64(buf, &mut i)?;
        let _lp_bps = read_u64(buf, &mut i)?;
        let _lp = read_u64(buf, &mut i)?;
        let _prot_bps = read_u64(buf, &mut i)?;
        let _prot = read_u64(buf, &mut i)?;
        let _qin_lp = read_u64(buf, &mut i)?;
        let user_quote_amount_in = read_u64(buf, &mut i)?;
        let pool = read_pubkey(buf, &mut i)?;
        let user = read_pubkey(buf, &mut i)?;
        return Ok(BuyEv {
            base_amount_out,
            user_quote_amount_in,
            pool,
            user,
        });
    }
    Err(EngineError::Malformed("BuyEvent not found".into()))
}

fn extract_sell_event(logs: &[String]) -> Result<SellEv> {
    for line in logs {
        let Some(bytes) = program_data(line) else {
            continue;
        };
        if !disc_eq(&bytes, &SELL_EVENT_DISCRIMINATOR) {
            continue;
        }
        let mut i = 0usize;
        let buf = &bytes[8..];
        let _ts = read_i64(buf, &mut i)?;
        let base_amount_in = read_u64(buf, &mut i)?;
        let _min = read_u64(buf, &mut i)?;
        let _ubr = read_u64(buf, &mut i)?;
        let _uqr = read_u64(buf, &mut i)?;
        let _pbr = read_u64(buf, &mut i)?;
        let _pqr = read_u64(buf, &mut i)?;
        let _qout = read_u64(buf, &mut i)?;
        let _lp_bps = read_u64(buf, &mut i)?;
        let _lp = read_u64(buf, &mut i)?;
        let _prot_bps = read_u64(buf, &mut i)?;
        let _prot = read_u64(buf, &mut i)?;
        let _qout_wo = read_u64(buf, &mut i)?;
        let user_quote_amount_out = read_u64(buf, &mut i)?;
        let pool = read_pubkey(buf, &mut i)?;
        let user = read_pubkey(buf, &mut i)?;
        return Ok(SellEv {
            base_amount_in,
            user_quote_amount_out,
            pool,
            user,
        });
    }
    Err(EngineError::Malformed("SellEvent not found".into()))
}
