use chrono::{DateTime, Utc};

use super::chain::Chain;
use crate::error::DatasetQualityError;

/// Explicit Solana ingest mode. Credentials never imply Yellowstone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolanaMode {
    Historical,
    RpcDev,
    Yellowstone,
}

impl SolanaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::RpcDev => "rpc_dev",
            Self::Yellowstone => "yellowstone",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "historical" | "history" | "fixture" | "replay" => Some(Self::Historical),
            "rpc_dev" | "rpcdev" | "rpc" | "dev" => Some(Self::RpcDev),
            "yellowstone" | "grpc" | "geyser" => Some(Self::Yellowstone),
            _ => None,
        }
    }

    /// CLI, then env, then rpc_dev. Never inferred from SOLANA_GRPC_URL.
    pub fn resolve(cli_mode: Option<&str>, env_mode: Option<&str>) -> Result<Self, String> {
        if let Some(raw) = cli_mode {
            return Self::parse(raw).ok_or_else(|| {
                format!("unknown Solana mode {raw}; use historical|rpc-dev|yellowstone")
            });
        }
        if let Some(raw) = env_mode {
            return Self::parse(raw).ok_or_else(|| {
                format!("unknown SOLANA_MODE={raw}; use historical|rpc_dev|yellowstone")
            });
        }
        Ok(Self::RpcDev)
    }

    pub fn from_env() -> Result<Self, String> {
        Self::resolve(None, std::env::var("SOLANA_MODE").ok().as_deref())
    }

    pub fn quality_status(self) -> QualityStatus {
        match self {
            Self::Historical => QualityStatus::HistoricalReplay,
            Self::RpcDev => QualityStatus::RpcDevIncomplete,
            Self::Yellowstone => QualityStatus::LiveComplete,
        }
    }

    pub fn session_complete_by_default(self) -> bool {
        !matches!(self, Self::RpcDev)
    }

    pub fn data_quality_label(self) -> &'static str {
        match self {
            Self::RpcDev => QualityStatus::DevelopmentIncomplete.as_str(),
            other => other.quality_status().as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QualityStatus {
    HistoricalReplay,
    HistoricalPartial,
    RpcDevIncomplete,
    LiveComplete,
    DevelopmentIncomplete,
    PartialMarketState,
}

impl QualityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HistoricalReplay => "HISTORICAL_REPLAY",
            Self::HistoricalPartial => "HISTORICAL_PARTIAL",
            Self::RpcDevIncomplete => "RPC_DEV_INCOMPLETE",
            Self::LiveComplete => "LIVE_COMPLETE",
            Self::DevelopmentIncomplete => "DEVELOPMENT_INCOMPLETE",
            Self::PartialMarketState => "PARTIAL_MARKET_STATE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "HISTORICAL_REPLAY" => Some(Self::HistoricalReplay),
            "HISTORICAL_PARTIAL" => Some(Self::HistoricalPartial),
            "RPC_DEV_INCOMPLETE" => Some(Self::RpcDevIncomplete),
            "LIVE_COMPLETE" => Some(Self::LiveComplete),
            "DEVELOPMENT_INCOMPLETE" => Some(Self::DevelopmentIncomplete),
            "PARTIAL_MARKET_STATE" => Some(Self::PartialMarketState),
            _ => None,
        }
    }

    pub fn is_research_complete(self) -> bool {
        matches!(self, Self::HistoricalReplay | Self::LiveComplete)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionSession {
    pub id: Option<i64>,
    pub chain: Chain,
    pub mode: String,
    pub provider: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub start_block: Option<i64>,
    pub end_block: Option<i64>,
    pub start_slot: Option<i64>,
    pub end_slot: Option<i64>,
    pub complete: bool,
    pub quality_status: QualityStatus,
    pub gap_count: i32,
    pub notes: Option<String>,
}

impl CollectionSession {
    pub fn start(
        chain: Chain,
        mode: SolanaMode,
        provider: impl Into<String>,
        notes: Option<String>,
    ) -> Self {
        let quality = mode.quality_status();
        Self {
            id: None,
            chain,
            mode: mode.as_str().to_string(),
            provider: provider.into(),
            started_at: Utc::now(),
            ended_at: None,
            start_block: None,
            end_block: None,
            start_slot: None,
            end_slot: None,
            complete: mode.session_complete_by_default(),
            quality_status: quality,
            gap_count: 0,
            notes,
        }
    }

    pub fn solana_mode(&self) -> Option<SolanaMode> {
        SolanaMode::parse(&self.mode)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QualityCheck {
    pub simulation_requires_complete_market_data: bool,
}

impl QualityCheck {
    pub fn complete_market_data() -> Self {
        Self {
            simulation_requires_complete_market_data: true,
        }
    }
}

/// Later research/simulation must call this before consuming a dataset.
pub fn validate_dataset_quality(
    session: &CollectionSession,
    check: QualityCheck,
) -> Result<(), DatasetQualityError> {
    if !check.simulation_requires_complete_market_data {
        return Ok(());
    }
    let mode = session.solana_mode();
    let incomplete = session.chain == Chain::Solana
        && (mode == Some(SolanaMode::RpcDev)
            || !session.complete
            || !session.quality_status.is_research_complete());
    if incomplete {
        metrics::counter!(
            "dataset_quality_rejection_total",
            "reason" => "incomplete_source"
        )
        .increment(1);
        return Err(DatasetQualityError::IncompleteSource {
            chain: session.chain.as_str().to_string(),
            mode: session.mode.clone(),
            status: session.quality_status.as_str().to_string(),
        });
    }
    Ok(())
}

pub const RPC_DEV_WARNING: &str = "WARNING:\nSolana rpc_dev mode is incomplete and must not be used for strategy performance evaluation.";
