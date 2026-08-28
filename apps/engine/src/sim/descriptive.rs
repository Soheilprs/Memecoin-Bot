//! DescriptiveTokenOutcome is NOT TokenOutcome. No execution fills. No PnL.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{
    Chain, DescriptiveLabelQuality, Launchpad, ResearchCapabilitySet, SLKY_DATASET_ID,
};

pub const DESCRIPTIVE_OUTCOME_VERSION: &str = "7.2.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DescriptiveTokenOutcome {
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub reference_time: DateTime<Utc>,
    pub reference_source_price: Option<String>,
    pub max_source_price_5m: Option<String>,
    pub max_source_price_15m: Option<String>,
    pub max_source_price_30m: Option<String>,
    pub max_source_price_1h: Option<String>,
    pub max_return_bps: Option<i64>,
    pub reached_2x: bool,
    pub reached_5x: bool,
    pub reached_10x: bool,
    pub reached_20x: bool,
    pub time_to_2x_ms: Option<i64>,
    pub time_to_5x_ms: Option<i64>,
    pub time_to_10x_ms: Option<i64>,
    pub time_to_20x_ms: Option<i64>,
    pub quality: DescriptiveLabelQuality,
    pub source: String,
    pub capabilities: ResearchCapabilitySet,
    #[serde(default)]
    pub maturity: OutcomeMaturity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutcomeMaturity {
    Pending,
    #[default]
    Mature,
    CensoredSessionEnd,
}

impl OutcomeMaturity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Mature => "MATURE",
            Self::CensoredSessionEnd => "CENSORED_SESSION_END",
        }
    }

    pub fn for_live_age(age_ms: i64) -> Self {
        if age_ms >= 3_600_000 {
            Self::Mature
        } else {
            Self::Pending
        }
    }
}

impl DescriptiveTokenOutcome {
    pub fn from_prices(
        token: impl Into<String>,
        reference_time: DateTime<Utc>,
        reference: Option<f64>,
        series: &[(i64, f64)],
    ) -> Self {
        let mut out = Self {
            chain: Chain::Solana,
            token: token.into(),
            launchpad: Launchpad::PumpFun,
            reference_time,
            reference_source_price: reference.map(|p| format!("{p}")),
            max_source_price_5m: None,
            max_source_price_15m: None,
            max_source_price_30m: None,
            max_source_price_1h: None,
            max_return_bps: None,
            reached_2x: false,
            reached_5x: false,
            reached_10x: false,
            reached_20x: false,
            time_to_2x_ms: None,
            time_to_5x_ms: None,
            time_to_10x_ms: None,
            time_to_20x_ms: None,
            quality: DescriptiveLabelQuality::Invalid,
            source: SLKY_DATASET_ID.into(),
            capabilities: ResearchCapabilitySet::slinky21_pump_corpus(false),
            maturity: OutcomeMaturity::Mature,
        };
        let Some(ref_px) = reference.filter(|p| *p > 0.0 && p.is_finite()) else {
            return out;
        };
        let mut mx5: f64 = 0.0;
        let mut mx15: f64 = 0.0;
        let mut mx30: f64 = 0.0;
        let mut mx1h: f64 = 0.0;
        for (age_ms, px) in series {
            if !px.is_finite() || *px <= 0.0 {
                continue;
            }
            if *age_ms <= 300_000 {
                mx5 = mx5.max(*px);
            }
            if *age_ms <= 900_000 {
                mx15 = mx15.max(*px);
            }
            if *age_ms <= 1_800_000 {
                mx30 = mx30.max(*px);
            }
            if *age_ms <= 3_600_000 {
                mx1h = mx1h.max(*px);
            }
            let ret = ((*px / ref_px) - 1.0) * 10_000.0;
            if ret.is_finite() {
                let bps = ret as i64;
                out.max_return_bps = Some(out.max_return_bps.unwrap_or(i64::MIN).max(bps));
                if bps >= 10_000 && out.time_to_2x_ms.is_none() {
                    out.time_to_2x_ms = Some(*age_ms);
                    out.reached_2x = true;
                }
                if bps >= 40_000 && out.time_to_5x_ms.is_none() {
                    out.time_to_5x_ms = Some(*age_ms);
                    out.reached_5x = true;
                }
                if bps >= 90_000 && out.time_to_10x_ms.is_none() {
                    out.time_to_10x_ms = Some(*age_ms);
                    out.reached_10x = true;
                }
                if bps >= 190_000 && out.time_to_20x_ms.is_none() {
                    out.time_to_20x_ms = Some(*age_ms);
                    out.reached_20x = true;
                }
            }
        }
        if mx5 > 0.0 {
            out.max_source_price_5m = Some(format!("{mx5}"));
        }
        if mx15 > 0.0 {
            out.max_source_price_15m = Some(format!("{mx15}"));
        }
        if mx30 > 0.0 {
            out.max_source_price_30m = Some(format!("{mx30}"));
        }
        if mx1h > 0.0 {
            out.max_source_price_1h = Some(format!("{mx1h}"));
        }
        out.quality = DescriptiveLabelQuality::DescriptiveHigh;
        out.capabilities.descriptive_outcome_valid = true;
        out
    }
}

/// Heartbeat/carry-forward rows are not trades.
pub fn is_heartbeat_row(prev: Option<&(i64, f64, u64)>, cur: &(i64, f64, u64)) -> bool {
    match prev {
        None => false,
        Some(p) => p.0 == cur.0 && (p.1 - cur.1).abs() < f64::EPSILON && p.2 == cur.2,
    }
}
