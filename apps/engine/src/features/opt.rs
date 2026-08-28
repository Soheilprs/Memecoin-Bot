//! Missing is not zero. Financial values stay decimal integer strings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeatureQuality {
    Value,
    Unknown,
    Partial,
}

impl FeatureQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Value => "VALUE",
            Self::Unknown => "UNKNOWN",
            Self::Partial => "PARTIAL",
        }
    }
}

/// Optional unsigned count. `Value(0)` is observed zero; `Unknown` is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "q", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptU64 {
    Value { v: u64 },
    Unknown,
    Partial { v: Option<u64> },
}

impl OptU64 {
    pub fn value(v: u64) -> Self {
        Self::Value { v }
    }
    pub fn unknown() -> Self {
        Self::Unknown
    }
    pub fn as_value(&self) -> Option<u64> {
        match self {
            Self::Value { v } => Some(*v),
            Self::Partial { v } => *v,
            Self::Unknown => None,
        }
    }
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "q", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptI64 {
    Value { v: i64 },
    Unknown,
}

impl OptI64 {
    pub fn value(v: i64) -> Self {
        Self::Value { v }
    }
    pub fn unknown() -> Self {
        Self::Unknown
    }
    pub fn as_value(&self) -> Option<i64> {
        match self {
            Self::Value { v } => Some(*v),
            Self::Unknown => None,
        }
    }
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Decimal integer string or unknown. Never f64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "q", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptAmt {
    Value { v: String },
    Unknown,
    Partial { v: Option<String> },
}

impl OptAmt {
    pub fn value(v: impl Into<String>) -> Self {
        Self::Value { v: v.into() }
    }
    pub fn unknown() -> Self {
        Self::Unknown
    }
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Basis-point ratio (n*10000/d). None if d==0 — never inf/NaN.
pub fn count_ratio_bps(n: u64, d: u64) -> Option<u32> {
    if d == 0 {
        return None;
    }
    Some(((n as u128 * 10_000) / d as u128).min(u32::MAX as u128) as u32)
}

pub fn imbalance_i64(buy: u64, sell: u64) -> i64 {
    buy as i64 - sell as i64
}
