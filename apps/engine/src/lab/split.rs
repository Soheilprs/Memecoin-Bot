//! Chronological train / validation / test splits. No random token shuffle.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SplitKind {
    Train,
    Validation,
    Test,
}

impl SplitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Train => "TRAIN",
            Self::Validation => "VALIDATION",
            Self::Test => "TEST",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitBounds {
    pub train_start: DateTime<Utc>,
    pub train_end: DateTime<Utc>,
    pub validation_start: DateTime<Utc>,
    pub validation_end: DateTime<Utc>,
    pub test_start: DateTime<Utc>,
    pub test_end: DateTime<Utc>,
}

/// 60 / 20 / 20 by time range, not by token count shuffle.
pub fn chronological_split(min_t: DateTime<Utc>, max_t: DateTime<Utc>) -> SplitBounds {
    let span = max_t.signed_duration_since(min_t).num_milliseconds().max(1);
    let t60 = min_t + chrono::Duration::milliseconds(span * 60 / 100);
    let t80 = min_t + chrono::Duration::milliseconds(span * 80 / 100);
    SplitBounds {
        train_start: min_t,
        train_end: t60,
        validation_start: t60,
        validation_end: t80,
        test_start: t80,
        test_end: max_t,
    }
}

pub fn assign_split(t: DateTime<Utc>, b: &SplitBounds) -> SplitKind {
    if t < b.validation_start {
        SplitKind::Train
    } else if t < b.test_start {
        SplitKind::Validation
    } else {
        SplitKind::Test
    }
}
