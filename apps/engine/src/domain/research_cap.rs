//! Explicit research capabilities. Descriptive work is not execution PnL.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResearchCapability {
    FeatureValid,
    DescriptiveOutcomeValid,
    ExecutionValid,
    PaperLiveValid,
    NonResearchValid,
}

impl ResearchCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FeatureValid => "FEATURE_VALID",
            Self::DescriptiveOutcomeValid => "DESCRIPTIVE_OUTCOME_VALID",
            Self::ExecutionValid => "EXECUTION_VALID",
            Self::PaperLiveValid => "PAPER_LIVE_VALID",
            Self::NonResearchValid => "NON_RESEARCH_VALID",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResearchCapabilitySet {
    pub feature_valid: bool,
    pub descriptive_outcome_valid: bool,
    pub execution_valid: bool,
    pub paper_live_valid: bool,
    pub non_research_valid: bool,
}

impl ResearchCapabilitySet {
    pub fn slinky21_pump_corpus(price_labels_ok: bool) -> Self {
        Self {
            feature_valid: true,
            descriptive_outcome_valid: price_labels_ok,
            execution_valid: false,
            paper_live_valid: false,
            non_research_valid: false,
        }
    }

    pub fn allows_strategy_pnl(self) -> bool {
        self.execution_valid || self.paper_live_valid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DescriptiveLabelQuality {
    DescriptiveHigh,
    DescriptivePartial,
    Invalid,
}

impl DescriptiveLabelQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DescriptiveHigh => "DESCRIPTIVE_HIGH",
            Self::DescriptivePartial => "DESCRIPTIVE_PARTIAL",
            Self::Invalid => "INVALID",
        }
    }

    pub fn usable_in_cohorts(self) -> bool {
        !matches!(self, Self::Invalid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupQuality {
    Complete,
    Partial,
    Incomplete,
}

impl GroupQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::Partial => "PARTIAL",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}
