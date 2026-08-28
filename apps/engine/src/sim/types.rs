//! Canonical simulation types. No live transactions. Money is integer decimal strings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad, QualityStatus};

pub const EXECUTION_MODEL_VERSION: &str = "6.0.0";
pub const FEE_MODEL_VERSION: &str = "6.0.0";
pub const IMPACT_MODEL_VERSION: &str = "6.0.0";
pub const FAILURE_MODEL_VERSION: &str = "6.0.0";
pub const OUTCOME_MODEL_VERSION: &str = "6.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SimulationMode {
    Historical,
    Paper,
}

impl SimulationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Historical => "HISTORICAL",
            Self::Paper => "PAPER",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LatencyScenario {
    Fast,
    Base,
    Slow,
}

impl LatencyScenario {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "FAST",
            Self::Base => "BASE",
            Self::Slow => "SLOW",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        match v.to_ascii_uppercase().as_str() {
            "FAST" => Some(Self::Fast),
            "BASE" => Some(Self::Base),
            "SLOW" => Some(Self::Slow),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    Filled,
    PartialFill,
    NoFill,
    Failed,
    RejectedLiquidity,
    RejectedSlippage,
    UnavailableMarketState,
    TemporarilyUnavailable,
    RejectedSecurity,
    RejectedQuality,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filled => "FILLED",
            Self::PartialFill => "PARTIAL_FILL",
            Self::NoFill => "NO_FILL",
            Self::Failed => "FAILED",
            Self::RejectedLiquidity => "REJECTED_LIQUIDITY",
            Self::RejectedSlippage => "REJECTED_SLIPPAGE",
            Self::UnavailableMarketState => "UNAVAILABLE_MARKET_STATE",
            Self::TemporarilyUnavailable => "TEMPORARILY_UNAVAILABLE",
            Self::RejectedSecurity => "REJECTED_SECURITY",
            Self::RejectedQuality => "REJECTED_QUALITY",
        }
    }

    pub fn is_fill(self) -> bool {
        matches!(self, Self::Filled | Self::PartialFill)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionQuality {
    Exact,
    Modelled,
    PartialState,
    NonResearchValid,
}

impl ExecutionQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::Modelled => "MODELLED",
            Self::PartialState => "PARTIAL_STATE",
            Self::NonResearchValid => "NON_RESEARCH_VALID",
        }
    }

    pub fn research_valid(self) -> bool {
        matches!(self, Self::Exact | Self::Modelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitReason {
    Stop,
    TakeProfit,
    Trail,
    MomentumDecay,
    CreatorSell,
    LiquidityDanger,
    TimeStop,
    StrategyExit,
    Emergency,
    SecurityEmergency,
    EndOfData,
    PartialScale,
    Unrealizable,
}

impl ExitReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "STOP",
            Self::TakeProfit => "TAKE_PROFIT",
            Self::Trail => "TRAIL",
            Self::MomentumDecay => "MOMENTUM_DECAY",
            Self::CreatorSell => "CREATOR_SELL",
            Self::LiquidityDanger => "LIQUIDITY_DANGER",
            Self::TimeStop => "TIME_STOP",
            Self::StrategyExit => "STRATEGY_EXIT",
            Self::Emergency => "EMERGENCY",
            Self::SecurityEmergency => "SECURITY_EMERGENCY",
            Self::EndOfData => "END_OF_DATA",
            Self::PartialScale => "PARTIAL_SCALE",
            Self::Unrealizable => "UNREALIZABLE",
        }
    }

    pub fn is_emergency(self) -> bool {
        matches!(
            self,
            Self::Emergency | Self::SecurityEmergency | Self::LiquidityDanger
        )
    }

    /// Audit label. Does not change policy semantics.
    pub fn audit_label(self, exit_policy_id: &str) -> &'static str {
        match self {
            Self::PartialScale => "PARTIAL_TAKE_PROFIT",
            Self::MomentumDecay => "FLOW_DECAY",
            Self::Trail => "TRAIL",
            Self::TimeStop if exit_policy_id.contains("X9") => "TIME_CAP",
            Self::TimeStop => "TIME_STOP",
            Self::CreatorSell => "CREATOR_EXIT",
            other => other.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionStatus {
    Open,
    Closed,
    ForcedEndOfData,
    Unrealizable,
    SessionEndedOpen,
}

impl PositionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Closed => "CLOSED",
            Self::ForcedEndOfData => "FORCED_END_OF_DATA",
            Self::Unrealizable => "UNREALIZABLE",
            Self::SessionEndedOpen => "SESSION_ENDED_OPEN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionEventKind {
    PositionOpened,
    PartialExit,
    TrailUpdated,
    StopUpdated,
    EmergencySignal,
    PositionClosed,
    ForcedEndOfData,
    ExitAttemptFailed,
}

impl PositionEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PositionOpened => "POSITION_OPENED",
            Self::PartialExit => "PARTIAL_EXIT",
            Self::TrailUpdated => "TRAIL_UPDATED",
            Self::StopUpdated => "STOP_UPDATED",
            Self::EmergencySignal => "EMERGENCY_SIGNAL",
            Self::PositionClosed => "POSITION_CLOSED",
            Self::ForcedEndOfData => "FORCED_END_OF_DATA",
            Self::ExitAttemptFailed => "EXIT_ATTEMPT_FAILED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRun {
    pub id: Option<i64>,
    pub mode: SimulationMode,
    pub chain: Option<Chain>,
    pub launchpad: Option<Launchpad>,
    pub strategy_policy_id: String,
    pub strategy_policy_version: String,
    pub execution_model_version: String,
    pub fee_model_version: String,
    pub impact_model_version: String,
    pub failure_model_version: String,
    pub source_session_id: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub data_quality: QualityStatus,
    pub research_valid: bool,
    pub config_snapshot: serde_json::Value,
    pub random_seed: u64,
    #[serde(default)]
    pub experiment_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl SimulationRun {
    pub fn new(
        mode: SimulationMode,
        policy_id: impl Into<String>,
        quality: QualityStatus,
        seed: u64,
        config: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            mode,
            chain: None,
            launchpad: None,
            strategy_policy_id: policy_id.into(),
            strategy_policy_version: "6.0.0".into(),
            execution_model_version: EXECUTION_MODEL_VERSION.into(),
            fee_model_version: FEE_MODEL_VERSION.into(),
            impact_model_version: IMPACT_MODEL_VERSION.into(),
            failure_model_version: FAILURE_MODEL_VERSION.into(),
            source_session_id: None,
            started_at: now,
            ended_at: None,
            data_quality: quality,
            research_valid: quality.is_research_complete(),
            config_snapshot: config,
            random_seed: seed,
            experiment_id: None,
            created_at: now,
        }
    }
}

pub fn quality_allows_research(q: QualityStatus) -> bool {
    q.is_research_complete()
}
