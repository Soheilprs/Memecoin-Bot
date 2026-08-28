use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    Found,
    NotFound,
    Pass,
    Warn,
    Fail,
    Unknown,
    UnknownHistoricalState,
    ProviderLimited,
    NotApplicable,
}

impl EvidenceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Found => "FOUND",
            Self::NotFound => "NOT_FOUND",
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
            Self::UnknownHistoricalState => "UNKNOWN_HISTORICAL_STATE",
            Self::ProviderLimited => "PROVIDER_LIMITED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityEvidence {
    pub check: String,
    pub status: EvidenceStatus,
    pub severity: Severity,
    pub hard_reject: bool,
    pub value: Option<String>,
    pub source: String,
    pub observed_at: DateTime<Utc>,
    pub as_of_block_or_slot: Option<String>,
    pub details: String,
}

impl SecurityEvidence {
    pub fn new(
        check: impl Into<String>,
        status: EvidenceStatus,
        severity: Severity,
        source: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            check: check.into(),
            status,
            severity,
            hard_reject: false,
            value: None,
            source: source.into(),
            observed_at: Utc::now(),
            as_of_block_or_slot: None,
            details: details.into(),
        }
    }

    pub fn with_value(mut self, v: impl Into<String>) -> Self {
        self.value = Some(v.into());
        self
    }

    pub fn reject(mut self) -> Self {
        self.hard_reject = true;
        self
    }

    pub fn at_slot(mut self, slot: u64) -> Self {
        self.as_of_block_or_slot = Some(format!("slot:{slot}"));
        self
    }

    pub fn at_block(mut self, block: u64) -> Self {
        self.as_of_block_or_slot = Some(format!("block:{block}"));
        self
    }
}
