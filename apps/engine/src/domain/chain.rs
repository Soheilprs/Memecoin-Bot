use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    Solana,
    Base,
    Robinhood,
}

impl Chain {
    pub fn as_str(self) -> &'static str {
        match self {
            Chain::Solana => "solana",
            Chain::Base => "base",
            Chain::Robinhood => "robinhood",
        }
    }

    pub fn evm_chain_id(self) -> Option<u64> {
        match self {
            Chain::Solana => None,
            Chain::Base => Some(8453),
            Chain::Robinhood => Some(4663),
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "solana" => Some(Chain::Solana),
            "base" => Some(Chain::Base),
            "robinhood" => Some(Chain::Robinhood),
            _ => None,
        }
    }

    pub fn from_evm_chain_id(id: u64) -> Option<Self> {
        match id {
            8453 => Some(Chain::Base),
            4663 => Some(Chain::Robinhood),
            _ => None,
        }
    }
}

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialOrd for Chain {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Chain {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}
