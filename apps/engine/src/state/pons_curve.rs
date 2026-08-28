//! Canonical Pons V2 curve state. Integer reserves only. No f64.

use alloy_primitives::U256;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::Chain;
use crate::state::amt::{parse_u256, u256_dec};
use crate::state::market::{BondingCurveState, MarketState, MarketStateQuality};

use super::snapshot::TokenStateSnapshot;

pub const PONS_CURVE_ABI_VERSION: &str = "v2-bondingcurve-getters-1";
pub const PONS_CURVE_SOURCE: &str =
    "https://github.com/ponsdotdev/ponsfamily/blob/main/contractsV2/src/v2/PonsV2BondingCurve.sol";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PonsCurveStateQuality {
    ExactBlockRead,
    LiveLatestRead,
    Reconstructed,
    Partial,
    Unknown,
}

impl PonsCurveStateQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactBlockRead => "EXACT_BLOCK_READ",
            Self::LiveLatestRead => "LIVE_LATEST_READ",
            Self::Reconstructed => "RECONSTRUCTED",
            Self::Partial => "PARTIAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn research_valid_live_paper(self) -> bool {
        matches!(self, Self::ExactBlockRead | Self::LiveLatestRead)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PonsCurveStatus {
    Active,
    ReadyToGraduate,
    Graduated,
    Unknown,
}

impl PonsCurveStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::ReadyToGraduate => "READY_TO_GRADUATE",
            Self::Graduated => "GRADUATED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PonsCurveState {
    pub chain: Chain,
    pub token: String,
    pub curve: String,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub virtual_quote_reserve: String,
    pub virtual_token_reserve: String,
    pub real_quote_reserve: String,
    pub real_token_reserve: String,
    pub quote_collected: String,
    pub graduation_threshold: String,
    pub progress_bps: Option<u32>,
    pub status: PonsCurveStatus,
    pub fee_bps: u32,
    pub creator_tax_bps: u32,
    pub snipe_tax_bps: Option<u32>,
    pub state_quality: PonsCurveStateQuality,
    pub source: String,
    pub abi_version: String,
}

impl PonsCurveState {
    pub fn to_bonding(&self) -> BondingCurveState {
        let quality = match self.state_quality {
            PonsCurveStateQuality::ExactBlockRead | PonsCurveStateQuality::LiveLatestRead => {
                MarketStateQuality::Complete
            }
            PonsCurveStateQuality::Reconstructed | PonsCurveStateQuality::Partial => {
                MarketStateQuality::Partial
            }
            PonsCurveStateQuality::Unknown => MarketStateQuality::Unknown,
        };
        BondingCurveState {
            virtual_token_reserves: Some(self.virtual_token_reserve.clone()),
            virtual_sol_reserves: Some(self.virtual_quote_reserve.clone()),
            real_token_reserves: Some(self.real_token_reserve.clone()),
            real_sol_reserves: Some(self.real_quote_reserve.clone()),
            token_total_supply: None,
            curve_progress_bps: self.progress_bps,
            last_token_amount_raw: None,
            last_quote_amount_raw: None,
            quality,
        }
    }

    pub fn is_tradeable(&self) -> bool {
        self.status == PonsCurveStatus::Active
            && parse_u256(&self.virtual_quote_reserve) > alloy_primitives::U256::ZERO
            && parse_u256(&self.virtual_token_reserve) > alloy_primitives::U256::ZERO
    }

    pub fn progress_from_reserves(real_quote: &str, threshold: &str) -> Option<u32> {
        let t = parse_u256(threshold);
        if t.is_zero() {
            return None;
        }
        let q = parse_u256(real_quote);
        let bps = q.saturating_mul(alloy_primitives::U256::from(10_000u64)) / t;
        u32::try_from(bps).ok().map(|v| v.min(10_000))
    }

    pub fn quote_fee_bps(&self) -> u32 {
        self.fee_bps.saturating_add(self.creator_tax_bps).min(2_000)
    }
}

pub fn parse_hex_u256(word: &str) -> U256 {
    let t = word
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if t.is_empty() {
        return U256::ZERO;
    }
    U256::from_str_radix(t, 16).unwrap_or(U256::ZERO)
}

pub fn u256_hex_word(word: &str) -> String {
    u256_dec(parse_hex_u256(word))
}

/// ABI-encoded eth_call result: 0x + 64 hex chars per word.
pub fn decode_abi_words(hex: &str) -> Vec<String> {
    let t = hex.trim().trim_start_matches("0x").trim_start_matches("0X");
    if t.is_empty() {
        return Vec::new();
    }
    t.as_bytes()
        .chunks(64)
        .filter(|c| c.len() == 64)
        .map(|c| u256_hex_word(&format!("0x{}", String::from_utf8_lossy(c))))
        .collect()
}

pub fn decode_abi_bool(hex: &str) -> bool {
    parse_hex_u256(hex) > U256::ZERO
}

pub fn overlay_snapshot(snap: &mut TokenStateSnapshot, state: &PonsCurveState) {
    snap.market_state = MarketState::BondingCurve(state.to_bonding());
    snap.market_state_type = "BONDING_CURVE".into();
    snap.curve_progress_bps = state.progress_bps;
    snap.graduation_progress_bps = state.progress_bps;
    if let Some(b) = state.block_number {
        snap.as_of_block = Some(b as i64);
    }
}
