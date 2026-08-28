use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandidateState {
    #[default]
    Discovered,
    SecurityPending,
    SecurityRejected,
    DataIncomplete,
    Watching,
    Confirming,
    Eligible,
    Expired,
}

impl CandidateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "DISCOVERED",
            Self::SecurityPending => "SECURITY_PENDING",
            Self::SecurityRejected => "SECURITY_REJECTED",
            Self::DataIncomplete => "DATA_INCOMPLETE",
            Self::Watching => "WATCHING",
            Self::Confirming => "CONFIRMING",
            Self::Eligible => "ELIGIBLE",
            Self::Expired => "EXPIRED",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "DISCOVERED" => Self::Discovered,
            "SECURITY_PENDING" => Self::SecurityPending,
            "SECURITY_REJECTED" => Self::SecurityRejected,
            "DATA_INCOMPLETE" => Self::DataIncomplete,
            "WATCHING" => Self::Watching,
            "CONFIRMING" => Self::Confirming,
            "ELIGIBLE" => Self::Eligible,
            "EXPIRED" => Self::Expired,
            _ => return None,
        })
    }

    pub fn is_tradeable_gate(self) -> bool {
        matches!(self, Self::Eligible)
    }
}
