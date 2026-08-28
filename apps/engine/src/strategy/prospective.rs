//! Predeclared Pons hypotheses from Solana descriptive research. Not RH-fitted.
//! PIPELINE_SMOKE_POLICY is plumbing only: research_valid_for_alpha = false.

use crate::candidate::CandidateState;
use crate::security::assessment::SecurityVerdict;
use crate::sim::models::SimConfig;
use crate::strategy::{StrategyContext, StrategyDecision};

pub fn smoke_decide(ctx: &StrategyContext<'_>, cfg: &SimConfig) -> StrategyDecision {
    if matches!(ctx.security, Some(SecurityVerdict::Reject)) {
        return StrategyDecision {
            enter: false,
            reason: "SECURITY_REJECT",
        };
    }
    if matches!(
        ctx.candidate,
        CandidateState::SecurityRejected | CandidateState::Expired | CandidateState::DataIncomplete
    ) {
        return StrategyDecision {
            enter: false,
            reason: "NOT_READY",
        };
    }
    if let Some(f) = ctx.features {
        if f.token_age_ms < cfg.fees.pons_snipe_window_ms.max(1_000) {
            return StrategyDecision {
                enter: false,
                reason: "PONS_SNIPE_WINDOW",
            };
        }
        if f.shared.is_graduation_gap {
            return StrategyDecision {
                enter: false,
                reason: "GRADUATION_GAP",
            };
        }
    }
    StrategyDecision {
        enter: true,
        reason: "PIPELINE_SMOKE_POLICY",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProspectivePolicy {
    id: &'static str,
}

impl ProspectivePolicy {
    pub fn all() -> [Self; 5] {
        [
            Self {
                id: "P0_FIRST_ELIGIBLE_CONTROL",
            },
            Self {
                id: "P1_SOLANA_BUYERS_3_30S",
            },
            Self {
                id: "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
            },
            Self {
                id: "P3_PRICE_WITHOUT_BUYERS_AVOID",
            },
            Self {
                id: "P4_LOW_PARTICIPATION_FILTER",
            },
        ]
    }

    pub fn id(self) -> &'static str {
        self.id
    }

    pub fn decide(self, ctx: &StrategyContext<'_>) -> StrategyDecision {
        if matches!(ctx.security, Some(SecurityVerdict::Reject)) {
            return StrategyDecision {
                enter: false,
                reason: "SECURITY_REJECT",
            };
        }
        if ctx.candidate != CandidateState::Eligible {
            return StrategyDecision {
                enter: false,
                reason: "NOT_ELIGIBLE",
            };
        }
        let Some(f) = ctx.features else {
            return StrategyDecision {
                enter: false,
                reason: "DATA_INCOMPLETE",
            };
        };
        let ub30 = f
            .shared
            .win30
            .unique_buyers
            .max(f.shared.unique_buyers_total);
        let imb = f.shared.trade_count_imbalance;
        let price_up = f
            .shared
            .price_change_30s_bps
            .as_value()
            .or_else(|| f.shared.price_change_15s_bps.as_value())
            .unwrap_or(0)
            > 0;
        let buyer_growth = f.shared.win30.new_unique_buyers > 0
            || f.shared
                .unique_buyer_acceleration_15s
                .as_value()
                .unwrap_or(0)
                > 0;
        match self.id {
            "P0_FIRST_ELIGIBLE_CONTROL" => StrategyDecision {
                enter: true,
                reason: self.id,
            },
            "P1_SOLANA_BUYERS_3_30S" => StrategyDecision {
                enter: ub30 >= 3,
                reason: self.id,
            },
            "P2_SOLANA_BUYERS_PLUS_IMBALANCE" => StrategyDecision {
                enter: buyer_growth && imb > 0,
                reason: self.id,
            },
            "P3_PRICE_WITHOUT_BUYERS_AVOID" => StrategyDecision {
                enter: !price_up || buyer_growth,
                reason: self.id,
            },
            "P4_LOW_PARTICIPATION_FILTER" => StrategyDecision {
                enter: ub30 >= 3,
                reason: self.id,
            },
            _ => StrategyDecision {
                enter: false,
                reason: "UNKNOWN_POLICY",
            },
        }
    }
}
