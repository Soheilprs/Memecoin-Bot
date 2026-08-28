//! Decoded research-corpus records. These are NOT raw chain transactions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::trade::TradeSide;

pub const SOURCE_KIND_DECODED_RESEARCH_CORPUS: &str = "DECODED_RESEARCH_CORPUS";
pub const NORMALIZATION_VERSION: &str = "7.1.0";
pub const IMPORTER_VERSION: &str = "7.1.0";
pub const SLKY_DATASET_ID: &str = "Slinky21/Pumpfun_Memecoin_Corpus";
pub const SLKY_SOURCE_URL: &str =
    "https://huggingface.co/datasets/Slinky21/Pumpfun_Memecoin_Corpus";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CorpusSourceKind {
    DecodedResearchCorpus,
}

impl CorpusSourceKind {
    pub fn as_str(self) -> &'static str {
        SOURCE_KIND_DECODED_RESEARCH_CORPUS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityQuality {
    OnchainExact,
    Derived,
}

impl IdentityQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnchainExact => "ONCHAIN_EXACT",
            Self::Derived => "DERIVED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AmountQuality {
    OnchainInteger,
    IntegerValuedFloat,
    FloatNotInteger,
    Missing,
    Inconsistent,
}

impl AmountQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnchainInteger => "ONCHAIN_INTEGER",
            Self::IntegerValuedFloat => "INTEGER_VALUED_FLOAT",
            Self::FloatNotInteger => "FLOAT_NOT_INTEGER",
            Self::Missing => "MISSING",
            Self::Inconsistent => "INCONSISTENT",
        }
    }

    pub fn execution_usable(self) -> bool {
        matches!(self, Self::OnchainInteger)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusEventType {
    Launch,
    Trade,
    Graduation,
}

impl CorpusEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Trade => "trade",
            Self::Graduation => "graduation",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "launch" | "create" | "token" => Some(Self::Launch),
            "trade" => Some(Self::Trade),
            "graduation" | "migrate" | "migration" => Some(Self::Graduation),
            _ => None,
        }
    }

    pub fn order_rank(self) -> u8 {
        match self {
            Self::Launch => 0,
            Self::Trade => 1,
            Self::Graduation => 2,
        }
    }
}

/// Tabular Pump.fun row adapted into the replay path. Provenance is decoded, not raw tx bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusRecord {
    #[serde(default = "default_source_kind")]
    pub source_kind: CorpusSourceKind,
    pub dataset_id: String,
    pub source_file: String,
    pub source_row: u64,
    pub event_type: CorpusEventType,
    #[serde(default = "default_identity_quality")]
    pub identity_quality: IdentityQuality,
    pub mint: String,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub trader: Option<String>,
    #[serde(default)]
    pub side: Option<TradeSide>,
    #[serde(default)]
    pub token_amount: Option<String>,
    #[serde(default)]
    pub sol_amount: Option<String>,
    #[serde(default = "default_amount_quality")]
    pub amount_quality: AmountQuality,
    pub timestamp: DateTime<Utc>,
    /// Whole milliseconds since launch when known. Never a silent zero.
    #[serde(default)]
    pub seconds_since_launch_milli: Option<i64>,
    #[serde(default)]
    pub slot: Option<u64>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub transaction_index: Option<u64>,
    #[serde(default)]
    pub instruction_index: Option<u32>,
    #[serde(default)]
    pub inner_instruction_index: Option<u32>,
    #[serde(default)]
    pub v_sol_bonding_curve: Option<String>,
    #[serde(default)]
    pub v_tokens_bonding_curve: Option<String>,
    #[serde(default)]
    pub data_quality: String,
    #[serde(default = "default_norm_version")]
    pub normalization_version: String,
    pub order_seq: u64,
    #[serde(default = "default_original")]
    pub original: serde_json::Value,
}

fn default_source_kind() -> CorpusSourceKind {
    CorpusSourceKind::DecodedResearchCorpus
}

fn default_identity_quality() -> IdentityQuality {
    IdentityQuality::Derived
}

fn default_amount_quality() -> AmountQuality {
    AmountQuality::Missing
}

fn default_norm_version() -> String {
    NORMALIZATION_VERSION.to_string()
}

fn default_original() -> serde_json::Value {
    serde_json::Value::Null
}

impl CorpusRecord {
    pub fn identity_string(&self) -> String {
        if self.identity_quality == IdentityQuality::OnchainExact {
            if let Some(sig) = self.signature.as_deref() {
                let inner = self
                    .inner_instruction_index
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let ix = self.instruction_index.unwrap_or(0);
                return format!("solana|{sig}|{ix}|{inner}");
            }
        }
        format!(
            "corpus|{}|{}|{}|{}|{}|{}",
            self.dataset_id,
            self.source_file,
            self.source_row,
            self.event_type.as_str(),
            self.mint,
            self.order_seq
        )
    }

    pub fn derived_tx_id(&self) -> String {
        if let Some(sig) = &self.signature {
            if !sig.is_empty() {
                return sig.clone();
            }
        }
        format!(
            "derived:{}:{}:{}",
            self.source_file, self.source_row, self.order_seq
        )
    }

    /// Deterministic order: time, type, mint, file, row, seq. Never random.
    pub fn order_key(&self) -> (i64, u8, String, String, u64, u64) {
        (
            self.timestamp.timestamp_millis(),
            self.event_type.order_rank(),
            self.mint.clone(),
            self.source_file.clone(),
            self.source_row,
            self.order_seq,
        )
    }
}

/// Classify a decimal string. Does not invent lamports from SOL floats.
pub fn classify_amount(raw: Option<&str>) -> (AmountQuality, Option<String>) {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty() && *s != "null") else {
        return (AmountQuality::Missing, None);
    };
    if s.chars().all(|c| c.is_ascii_digit()) {
        return (AmountQuality::OnchainInteger, Some(s.to_string()));
    }
    if let Some(stripped) = s.strip_suffix(".0") {
        if !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_digit()) {
            return (
                AmountQuality::IntegerValuedFloat,
                Some(stripped.to_string()),
            );
        }
    }
    if s.parse::<f64>().is_ok() {
        return (AmountQuality::FloatNotInteger, Some(s.to_string()));
    }
    (AmountQuality::Missing, Some(s.to_string()))
}
