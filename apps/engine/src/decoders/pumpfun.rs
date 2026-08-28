use base64::Engine as _;
use chrono::{TimeZone, Utc};

use crate::domain::{
    CanonicalEvent, CanonicalStatus, Chain, GraduationModel, LaunchMechanism, Launchpad,
    LifecycleObserved, LifecycleType, RawEvent, TokenDiscovered, TradeObserved, TradeSide,
};
use crate::error::{EngineError, Result};
use crate::registry::{PUMPFUN_IDL_VERSION, PUMPFUN_PROGRAM, SOL_MINT};

use super::solana_buf::{
    decode_ix_data, disc_eq, read_bool, read_i64, read_pubkey, read_string, read_u16, read_u32,
    read_u64,
};
use super::Decoder;

pub const CREATE_DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];
pub const CREATE_V2_DISCRIMINATOR: [u8; 8] = [214, 144, 76, 236, 95, 139, 49, 180];
pub const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
pub const BUY_V2_DISCRIMINATOR: [u8; 8] = [184, 23, 238, 97, 103, 197, 211, 61];
pub const BUY_EXACT_SOL_IN_DISCRIMINATOR: [u8; 8] = [56, 252, 116, 8, 158, 223, 205, 95];
pub const BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR: [u8; 8] = [194, 171, 28, 70, 104, 77, 91, 47];
pub const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
pub const SELL_V2_DISCRIMINATOR: [u8; 8] = [93, 246, 130, 60, 231, 233, 64, 178];
pub const MIGRATE_DISCRIMINATOR: [u8; 8] = [155, 234, 231, 146, 236, 158, 162, 30];
pub const MIGRATE_V2_DISCRIMINATOR: [u8; 8] = [187, 203, 18, 31, 206, 237, 254, 41];

pub const CREATE_EVENT_DISCRIMINATOR: [u8; 8] = [27, 114, 169, 77, 222, 235, 99, 118];
pub const TRADE_EVENT_DISCRIMINATOR: [u8; 8] = [189, 219, 127, 211, 78, 230, 97, 238];
pub const COMPLETE_EVENT_DISCRIMINATOR: [u8; 8] = [95, 114, 97, 156, 212, 46, 152, 8];
pub const COMPLETE_PUMP_AMM_MIGRATION_DISCRIMINATOR: [u8; 8] =
    [189, 233, 93, 185, 92, 148, 234, 148];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpIxKind {
    Create,
    CreateV2,
    Buy,
    Sell,
    Migrate,
}

pub fn classify_pump_ix(data: &[u8]) -> Option<PumpIxKind> {
    if data.len() < 8 {
        return None;
    }
    let d: [u8; 8] = data[..8].try_into().ok()?;
    Some(match d {
        CREATE_DISCRIMINATOR | CREATE_V2_DISCRIMINATOR => {
            if d == CREATE_V2_DISCRIMINATOR {
                PumpIxKind::CreateV2
            } else {
                PumpIxKind::Create
            }
        }
        BUY_DISCRIMINATOR
        | BUY_V2_DISCRIMINATOR
        | BUY_EXACT_SOL_IN_DISCRIMINATOR
        | BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR => PumpIxKind::Buy,
        SELL_DISCRIMINATOR | SELL_V2_DISCRIMINATOR => PumpIxKind::Sell,
        MIGRATE_DISCRIMINATOR | MIGRATE_V2_DISCRIMINATOR => PumpIxKind::Migrate,
        _ => return None,
    })
}

pub struct PumpfunDecoder {
    version: &'static str,
}

impl PumpfunDecoder {
    pub fn pinned() -> Self {
        Self {
            version: PUMPFUN_IDL_VERSION,
        }
    }

    pub fn with_version(version: &'static str) -> Self {
        Self { version }
    }
}

impl Decoder for PumpfunDecoder {
    fn name(&self) -> &'static str {
        "pumpfun"
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn matches(&self, raw: &RawEvent) -> bool {
        let Some(ix) = raw.as_solana() else {
            return false;
        };
        if ix.program_id != PUMPFUN_PROGRAM {
            return false;
        }
        match decode_ix_data(&ix.data_base58) {
            Ok(bytes) => classify_pump_ix(&bytes).is_some(),
            _ => false,
        }
    }

    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
        if self.version != PUMPFUN_IDL_VERSION {
            return Err(EngineError::DecoderVersionMismatch {
                protocol: self.name().to_string(),
                requested: self.version.to_string(),
                pinned: PUMPFUN_IDL_VERSION.to_string(),
            });
        }
        let ix = raw.as_solana().ok_or_else(|| {
            EngineError::DecoderMismatch("pumpfun decoder requires solana instruction".into())
        })?;
        let data = decode_ix_data(&ix.data_base58)?;
        let kind = classify_pump_ix(&data).ok_or_else(|| {
            EngineError::DecoderMismatch("not a tracked pumpfun instruction".into())
        })?;
        if ix.execution_status == crate::domain::ExecutionStatus::Failed {
            match kind {
                PumpIxKind::Buy | PumpIxKind::Sell => return Ok(Vec::new()),
                _ => {}
            }
        }

        match kind {
            PumpIxKind::Create | PumpIxKind::CreateV2 => decode_create(self, raw, kind),
            PumpIxKind::Buy | PumpIxKind::Sell => decode_trade(self, raw, kind),
            PumpIxKind::Migrate => decode_migrate(self, raw),
        }
    }
}

fn decode_create(
    decoder: &PumpfunDecoder,
    raw: &RawEvent,
    kind: PumpIxKind,
) -> Result<Vec<CanonicalEvent>> {
    let ix = raw.as_solana().unwrap();
    let event = extract_create_event(&ix.log_messages)?;
    let chain_timestamp = event
        .timestamp
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .or(ix.block_time);
    let token = TokenDiscovered {
        chain: Chain::Solana,
        chain_id: None,
        token_address: event.mint.clone(),
        creator: event.creator.clone(),
        launchpad: Launchpad::PumpFun,
        factory_or_program: PUMPFUN_PROGRAM.to_string(),
        pool: None,
        curve: Some(event.bonding_curve.clone()),
        quote_asset: Some(event.quote_mint.clone()),
        launch_mechanism: LaunchMechanism::BondingCurve,
        bonding_curve: true,
        graduation_model: GraduationModel::PumpAmm,
        block_number: None,
        block_hash: None,
        slot: ix.slot,
        tx_hash_or_signature: ix.signature.clone(),
        instruction_index: Some(ix.instruction_index),
        inner_instruction_index: ix.inner_instruction_index,
        log_index: None,
        chain_timestamp,
        observed_at: raw.observed_at,
        persisted_at: None,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        initial_liquidity: None,
        raw_event_id: raw.event_id(),
    };
    let life = lifecycle_from_raw(
        decoder,
        raw,
        &event.mint,
        LifecycleType::TokenCreated,
        None,
        Some(event.bonding_curve),
        serde_json::json!({
            "instruction": if kind == PumpIxKind::CreateV2 { "create_v2" } else { "create" },
            "creator": event.creator,
            "quote_mint": event.quote_mint,
        }),
    );
    Ok(vec![
        CanonicalEvent::TokenDiscovered(Box::new(token)),
        CanonicalEvent::Lifecycle(Box::new(life)),
    ])
}

fn decode_trade(
    decoder: &PumpfunDecoder,
    raw: &RawEvent,
    kind: PumpIxKind,
) -> Result<Vec<CanonicalEvent>> {
    let ix = raw.as_solana().unwrap();
    let trade = extract_trade_event(&ix.log_messages)?;
    let chain_timestamp = Utc
        .timestamp_opt(trade.timestamp, 0)
        .single()
        .or(ix.block_time);
    let side = if trade.is_buy {
        TradeSide::Buy
    } else {
        TradeSide::Sell
    };
    let quote_asset = if trade.quote_mint.is_empty() {
        SOL_MINT.to_string()
    } else {
        trade.quote_mint.clone()
    };
    let quote_amount = if trade.quote_amount > 0 {
        trade.quote_amount
    } else {
        trade.sol_amount
    };
    let observed = TradeObserved {
        event_id: raw.event_id(),
        chain: Chain::Solana,
        launchpad: Launchpad::PumpFun,
        token_address: trade.mint.clone(),
        trader: trade.user.clone(),
        side,
        base_amount_raw: trade.token_amount.to_string(),
        quote_amount_raw: quote_amount.to_string(),
        base_decimals: 6,
        quote_decimals: 9,
        quote_asset,
        pool: None,
        curve: ix.accounts.get(3).cloned(),
        price_estimate: None,
        block_number: None,
        block_hash: None,
        slot: ix.slot,
        transaction_index: ix.transaction_index.map(|v| v as u64),
        tx_hash_or_signature: ix.signature.clone(),
        log_index: None,
        instruction_index: Some(ix.instruction_index),
        inner_instruction_index: ix.inner_instruction_index,
        chain_timestamp,
        observed_at: raw.observed_at,
        persisted_at: None,
        canonical_status: raw.canonical_status,
        finality: raw.finality,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata: serde_json::json!({
            "instruction": match kind {
                PumpIxKind::Buy => trade.ix_name.clone(),
                PumpIxKind::Sell => trade.ix_name.clone(),
                _ => trade.ix_name.clone(),
            },
            "sol_amount": trade.sol_amount.to_string(),
            "virtual_sol_reserves": trade.virtual_sol_reserves.to_string(),
            "virtual_token_reserves": trade.virtual_token_reserves.to_string(),
            "real_sol_reserves": trade.real_sol_reserves.to_string(),
            "real_token_reserves": trade.real_token_reserves.to_string(),
            "fee": trade.fee.to_string(),
            "creator_fee": trade.creator_fee.to_string(),
        }),
    };
    Ok(vec![CanonicalEvent::Trade(Box::new(observed))])
}

fn decode_migrate(decoder: &PumpfunDecoder, raw: &RawEvent) -> Result<Vec<CanonicalEvent>> {
    let ix = raw.as_solana().unwrap();
    let complete = extract_complete_pump_amm(&ix.log_messages).ok();
    let simple = extract_complete_event(&ix.log_messages).ok();
    let (mint, bonding_curve, pool, extra) = if let Some(c) = complete {
        (
            c.mint.clone(),
            Some(c.bonding_curve.clone()),
            Some(c.pool.clone()),
            serde_json::json!({
                "instruction": "migrate",
                "user": c.user,
                "mint_amount": c.mint_amount.to_string(),
                "sol_amount": c.sol_amount.to_string(),
                "pool": c.pool,
                "quote_mint": c.quote_mint,
            }),
        )
    } else if let Some(c) = simple {
        (
            c.mint.clone(),
            Some(c.bonding_curve.clone()),
            None,
            serde_json::json!({
                "instruction": "migrate",
                "user": c.user,
                "quote_mint": c.quote_mint,
            }),
        )
    } else {
        let mint = ix
            .accounts
            .get(2)
            .cloned()
            .ok_or_else(|| EngineError::Malformed("migrate missing mint account".into()))?;
        (
            mint,
            ix.accounts.get(3).cloned(),
            ix.accounts.get(9).cloned(),
            serde_json::json!({ "instruction": "migrate", "event": "missing_program_data" }),
        )
    };
    let life = lifecycle_from_raw(
        decoder,
        raw,
        &mint,
        LifecycleType::Migrated,
        pool,
        bonding_curve,
        extra,
    );
    Ok(vec![CanonicalEvent::Lifecycle(Box::new(life))])
}

fn lifecycle_from_raw(
    decoder: &PumpfunDecoder,
    raw: &RawEvent,
    token: &str,
    kind: LifecycleType,
    pool: Option<String>,
    curve: Option<String>,
    metadata: serde_json::Value,
) -> LifecycleObserved {
    let ix = raw.as_solana().unwrap();
    LifecycleObserved {
        event_id: raw.event_id(),
        chain: Chain::Solana,
        launchpad: Launchpad::PumpFun,
        token_address: token.to_string(),
        lifecycle_type: kind,
        factory: Some(PUMPFUN_PROGRAM.to_string()),
        pool,
        curve,
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
        canonical_status: CanonicalStatus::Canonical,
        finality: raw.finality,
        source: raw.source.clone(),
        decoder_version: decoder.version.to_string(),
        raw_event_id: raw.event_id(),
        metadata,
    }
}

struct CreateEvent {
    mint: String,
    bonding_curve: String,
    creator: String,
    timestamp: Option<i64>,
    quote_mint: String,
}

struct TradeEvent {
    mint: String,
    sol_amount: u64,
    token_amount: u64,
    is_buy: bool,
    user: String,
    timestamp: i64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
    real_sol_reserves: u64,
    real_token_reserves: u64,
    fee: u64,
    creator_fee: u64,
    ix_name: String,
    quote_mint: String,
    quote_amount: u64,
}

struct CompletePumpAmm {
    user: String,
    mint: String,
    mint_amount: u64,
    sol_amount: u64,
    bonding_curve: String,
    pool: String,
    quote_mint: String,
}

struct CompleteSimple {
    user: String,
    mint: String,
    bonding_curve: String,
    quote_mint: String,
}

fn extract_create_event(logs: &[String]) -> Result<CreateEvent> {
    for line in logs {
        let Some(bytes) = program_data_bytes(line) else {
            continue;
        };
        if !disc_eq(&bytes, &CREATE_EVENT_DISCRIMINATOR) {
            continue;
        }
        return parse_create_event_body(&bytes[8..]);
    }
    Err(EngineError::Malformed(
        "pumpfun CreateEvent program data log not found".into(),
    ))
}

fn extract_trade_event(logs: &[String]) -> Result<TradeEvent> {
    for line in logs {
        let Some(bytes) = program_data_bytes(line) else {
            continue;
        };
        if !disc_eq(&bytes, &TRADE_EVENT_DISCRIMINATOR) {
            continue;
        }
        return parse_trade_event_body(&bytes[8..]);
    }
    Err(EngineError::Malformed(
        "pumpfun TradeEvent program data log not found".into(),
    ))
}

fn extract_complete_pump_amm(logs: &[String]) -> Result<CompletePumpAmm> {
    for line in logs {
        let Some(bytes) = program_data_bytes(line) else {
            continue;
        };
        if !disc_eq(&bytes, &COMPLETE_PUMP_AMM_MIGRATION_DISCRIMINATOR) {
            continue;
        }
        return parse_complete_pump_amm(&bytes[8..]);
    }
    Err(EngineError::Malformed(
        "CompletePumpAmmMigrationEvent not found".into(),
    ))
}

fn extract_complete_event(logs: &[String]) -> Result<CompleteSimple> {
    for line in logs {
        let Some(bytes) = program_data_bytes(line) else {
            continue;
        };
        if !disc_eq(&bytes, &COMPLETE_EVENT_DISCRIMINATOR) {
            continue;
        }
        return parse_complete_simple(&bytes[8..]);
    }
    Err(EngineError::Malformed("CompleteEvent not found".into()))
}

fn program_data_bytes(line: &str) -> Option<Vec<u8>> {
    let b64 = line.strip_prefix("Program data: ")?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()
}

fn parse_create_event_body(buf: &[u8]) -> Result<CreateEvent> {
    let mut i = 0usize;
    let _name = read_string(buf, &mut i)?;
    let _symbol = read_string(buf, &mut i)?;
    let _uri = read_string(buf, &mut i)?;
    let mint = read_pubkey(buf, &mut i)?;
    let bonding_curve = read_pubkey(buf, &mut i)?;
    let _user = read_pubkey(buf, &mut i)?;
    let creator = read_pubkey(buf, &mut i)?;
    let timestamp = read_i64(buf, &mut i)?;
    let _virtual_token = read_u64(buf, &mut i)?;
    let _virtual_sol = read_u64(buf, &mut i)?;
    let _real_token = read_u64(buf, &mut i)?;
    let _supply = read_u64(buf, &mut i)?;
    let _token_program = read_pubkey(buf, &mut i)?;
    let _mayhem = read_bool(buf, &mut i)?;
    let _cashback = read_bool(buf, &mut i)?;
    let quote_mint = read_pubkey(buf, &mut i)?;
    let _virtual_quote = read_u64(buf, &mut i)?;
    Ok(CreateEvent {
        mint,
        bonding_curve,
        creator,
        timestamp: Some(timestamp),
        quote_mint,
    })
}

fn parse_trade_event_body(buf: &[u8]) -> Result<TradeEvent> {
    let mut i = 0usize;
    let mint = read_pubkey(buf, &mut i)?;
    let sol_amount = read_u64(buf, &mut i)?;
    let token_amount = read_u64(buf, &mut i)?;
    let is_buy = read_bool(buf, &mut i)?;
    let user = read_pubkey(buf, &mut i)?;
    let timestamp = read_i64(buf, &mut i)?;
    let virtual_sol_reserves = read_u64(buf, &mut i)?;
    let virtual_token_reserves = read_u64(buf, &mut i)?;
    let real_sol_reserves = read_u64(buf, &mut i)?;
    let real_token_reserves = read_u64(buf, &mut i)?;
    let _fee_recipient = read_pubkey(buf, &mut i)?;
    let _fee_bps = read_u64(buf, &mut i)?;
    let fee = read_u64(buf, &mut i)?;
    let _creator = read_pubkey(buf, &mut i)?;
    let _creator_fee_bps = read_u64(buf, &mut i)?;
    let creator_fee = read_u64(buf, &mut i)?;
    let _track_volume = read_bool(buf, &mut i)?;
    let _total_unclaimed = read_u64(buf, &mut i)?;
    let _total_claimed = read_u64(buf, &mut i)?;
    let _current_sol_volume = read_u64(buf, &mut i)?;
    let _last_update = read_i64(buf, &mut i)?;
    let ix_name =
        read_string(buf, &mut i).unwrap_or_else(
            |_| {
                if is_buy {
                    "buy".into()
                } else {
                    "sell".into()
                }
            },
        );
    let mut quote_mint = SOL_MINT.to_string();
    let mut quote_amount = sol_amount;
    if let Ok(_mayhem) = read_bool(buf, &mut i) {
        let _ = read_u64(buf, &mut i);
        let _ = read_u64(buf, &mut i);
        let _ = read_u64(buf, &mut i);
        let _ = read_u64(buf, &mut i);
        if let Ok(n) = read_u32(buf, &mut i) {
            for _ in 0..n {
                let _ = read_pubkey(buf, &mut i);
                let _ = read_u16(buf, &mut i);
            }
        }
        if let Ok(qm) = read_pubkey(buf, &mut i) {
            quote_mint = qm;
        }
        if let Ok(qa) = read_u64(buf, &mut i) {
            quote_amount = qa;
        }
    }
    Ok(TradeEvent {
        mint,
        sol_amount,
        token_amount,
        is_buy,
        user,
        timestamp,
        virtual_sol_reserves,
        virtual_token_reserves,
        real_sol_reserves,
        real_token_reserves,
        fee,
        creator_fee,
        ix_name,
        quote_mint,
        quote_amount,
    })
}

fn parse_complete_pump_amm(buf: &[u8]) -> Result<CompletePumpAmm> {
    let mut i = 0usize;
    let user = read_pubkey(buf, &mut i)?;
    let mint = read_pubkey(buf, &mut i)?;
    let mint_amount = read_u64(buf, &mut i)?;
    let sol_amount = read_u64(buf, &mut i)?;
    let _pool_migration_fee = read_u64(buf, &mut i)?;
    let bonding_curve = read_pubkey(buf, &mut i)?;
    let _timestamp = read_i64(buf, &mut i)?;
    let pool = read_pubkey(buf, &mut i)?;
    let quote_mint = read_pubkey(buf, &mut i).unwrap_or_else(|_| SOL_MINT.to_string());
    Ok(CompletePumpAmm {
        user,
        mint,
        mint_amount,
        sol_amount,
        bonding_curve,
        pool,
        quote_mint,
    })
}

fn parse_complete_simple(buf: &[u8]) -> Result<CompleteSimple> {
    let mut i = 0usize;
    let user = read_pubkey(buf, &mut i)?;
    let mint = read_pubkey(buf, &mut i)?;
    let bonding_curve = read_pubkey(buf, &mut i)?;
    let _timestamp = read_i64(buf, &mut i)?;
    let quote_mint = read_pubkey(buf, &mut i).unwrap_or_else(|_| SOL_MINT.to_string());
    Ok(CompleteSimple {
        user,
        mint,
        bonding_curve,
        quote_mint,
    })
}
