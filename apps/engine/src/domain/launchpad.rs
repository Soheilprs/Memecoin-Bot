use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Launchpad {
    PumpFun,
    PumpSwap,
    PonsV2,
    ClankerV4,
    Unknown,
}

impl Launchpad {
    pub fn as_str(self) -> &'static str {
        match self {
            Launchpad::PumpFun => "pumpfun",
            Launchpad::PumpSwap => "pumpswap",
            Launchpad::PonsV2 => "pons_v2",
            Launchpad::ClankerV4 => "clanker_v4",
            Launchpad::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "pumpfun" | "pump_fun" => Launchpad::PumpFun,
            "pumpswap" | "pump_swap" => Launchpad::PumpSwap,
            "pons_v2" => Launchpad::PonsV2,
            "clanker_v4" => Launchpad::ClankerV4,
            _ => Launchpad::Unknown,
        }
    }
}

impl std::fmt::Display for Launchpad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMechanism {
    BondingCurve,
    LockedV4,
    Unknown,
}

impl LaunchMechanism {
    pub fn as_str(self) -> &'static str {
        match self {
            LaunchMechanism::BondingCurve => "bonding_curve",
            LaunchMechanism::LockedV4 => "locked_v4",
            LaunchMechanism::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationModel {
    PumpAmm,
    PonsV4Hook,
    None,
    Unknown,
}

impl GraduationModel {
    pub fn as_str(self) -> &'static str {
        match self {
            GraduationModel::PumpAmm => "pump_amm",
            GraduationModel::PonsV4Hook => "pons_v4_hook",
            GraduationModel::None => "none",
            GraduationModel::Unknown => "unknown",
        }
    }
}
