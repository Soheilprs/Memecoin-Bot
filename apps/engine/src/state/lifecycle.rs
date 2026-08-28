use serde::{Deserialize, Serialize};

use crate::domain::Launchpad;

/// Protocol-aware token lifecycle. Not every protocol walks every variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenLifecycleState {
    #[default]
    Discovered,
    CurveActive,
    MigrationPending,
    Migrating,
    LaunchSwept,
    GraduationGap,
    AmmActive,
    Inactive,
    /// Reserved for Phase 4. Sparse outcome snapshots still apply.
    RejectedSecurity,
}

impl TokenLifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "DISCOVERED",
            Self::CurveActive => "CURVE_ACTIVE",
            Self::MigrationPending => "MIGRATION_PENDING",
            Self::Migrating => "MIGRATING",
            Self::LaunchSwept => "LAUNCH_SWEPT",
            Self::GraduationGap => "GRADUATION_GAP",
            Self::AmmActive => "AMM_ACTIVE",
            Self::Inactive => "INACTIVE",
            Self::RejectedSecurity => "REJECTED_SECURITY",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "DISCOVERED" => Self::Discovered,
            "CURVE_ACTIVE" => Self::CurveActive,
            "MIGRATION_PENDING" => Self::MigrationPending,
            "MIGRATING" => Self::Migrating,
            "LAUNCH_SWEPT" => Self::LaunchSwept,
            "GRADUATION_GAP" => Self::GraduationGap,
            "AMM_ACTIVE" => Self::AmmActive,
            "INACTIVE" => Self::Inactive,
            "REJECTED_SECURITY" => Self::RejectedSecurity,
            _ => return None,
        })
    }

    pub fn initial(launchpad: Launchpad) -> Self {
        match launchpad {
            Launchpad::ClankerV4 => Self::AmmActive,
            Launchpad::PumpFun | Launchpad::PonsV2 => Self::Discovered,
            Launchpad::PumpSwap => Self::AmmActive,
            Launchpad::Unknown => Self::Discovered,
        }
    }
}
