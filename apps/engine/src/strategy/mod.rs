//! Interpretable entry strategies. Runtime must not import outcome labels or future returns.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::candidate::CandidateState;
use crate::features::opt::{OptI64, OptU64};
use crate::features::vector::{FeatureVector, ProtocolFeatures};
use crate::security::assessment::SecurityVerdict;
use crate::sim::policy::{may_enter, EntryPolicyId};
use crate::state::amt::parse_u256;

pub mod prospective;

pub const STRATEGY_POLICY_VERSION: &str = "7.0.0";

pub use prospective::{smoke_decide, ProspectivePolicy};

#[derive(Debug, Clone)]
pub struct StrategyContext<'a> {
    pub features: Option<&'a FeatureVector>,
    pub candidate: CandidateState,
    pub security: Option<SecurityVerdict>,
    pub first_eligible_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub token: &'a str,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyDecision {
    pub enter: bool,
    pub reason: &'static str,
}

pub trait EntryStrategy: Send + Sync {
    fn id(&self) -> &'static str;
    fn decide(&self, ctx: &StrategyContext<'_>) -> StrategyDecision;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyThresholds {
    pub min_buyer_accel_15s: i64,
    pub min_buyer_velocity_15s: i64,
    pub min_unique_buyers: u64,
    pub max_repeat_buyer_ratio_bps: Option<u32>,
    pub min_new_buyer_ratio_30s_bps: Option<u32>,
    pub min_net_flow_non_negative: bool,
    pub reject_creator_sold: bool,
    pub min_curve_progress_bps: Option<u64>,
    pub max_curve_progress_bps: Option<u64>,
    pub min_age_ms: i64,
    pub baseline: Option<String>,
}

impl StrategyThresholds {
    pub fn train_defaults() -> Self {
        Self {
            min_buyer_accel_15s: 1,
            min_buyer_velocity_15s: 2,
            min_unique_buyers: 3,
            max_repeat_buyer_ratio_bps: Some(6_000),
            min_new_buyer_ratio_30s_bps: Some(2_000),
            min_net_flow_non_negative: true,
            reject_creator_sold: false,
            min_curve_progress_bps: Some(200),
            max_curve_progress_bps: Some(8_000),
            min_age_ms: 15_000,
            baseline: None,
        }
    }

    pub fn from_train_quantile(accel_q50: i64, vel_q50: i64, buyers_q50: u64) -> Self {
        let mut t = Self::train_defaults();
        t.min_buyer_accel_15s = accel_q50.max(1);
        t.min_buyer_velocity_15s = vel_q50.max(1);
        t.min_unique_buyers = buyers_q50.max(2);
        t
    }
}

pub fn quantile_i64(mut xs: Vec<i64>, q: u8) -> Option<i64> {
    if xs.is_empty() {
        return None;
    }
    xs.sort_unstable();
    let q = q.min(100) as usize;
    let idx = (xs.len() - 1) * q / 100;
    Some(xs[idx])
}

pub fn quantile_u64(mut xs: Vec<u64>, q: u8) -> Option<u64> {
    if xs.is_empty() {
        return None;
    }
    xs.sort_unstable();
    let q = q.min(100) as usize;
    let idx = (xs.len() - 1) * q / 100;
    Some(xs[idx])
}

fn gate_security_eligible(ctx: &StrategyContext<'_>) -> Result<(), &'static str> {
    match ctx.security {
        Some(SecurityVerdict::Reject) => return Err("SECURITY_REJECT"),
        Some(SecurityVerdict::Unknown) | None => return Err("SECURITY_UNKNOWN"),
        Some(SecurityVerdict::Pass) | Some(SecurityVerdict::Warn) => {}
    }
    if ctx.candidate != CandidateState::Eligible {
        return Err("NOT_ELIGIBLE");
    }
    Ok(())
}

fn opt_i(v: &OptI64) -> Option<i64> {
    v.as_value()
}
fn opt_u(v: &OptU64) -> Option<u64> {
    v.as_value()
}

fn net_negative(s: &str) -> bool {
    s.trim_start().starts_with('-')
        && parse_u256(s.trim_start_matches('-')) > alloy_primitives::U256::ZERO
}

fn curve_bps(f: &FeatureVector) -> Option<u64> {
    match &f.protocol {
        ProtocolFeatures::SolanaPump {
            curve_progress_bps, ..
        } => opt_u(curve_progress_bps),
        ProtocolFeatures::RobinhoodPons {
            graduation_progress_bps,
            ..
        } => opt_u(graduation_progress_bps),
        _ => f.shared.current_progress_to_graduation_bps.as_value(),
    }
}

pub struct BaselineEntry {
    pub policy: EntryPolicyId,
}

impl EntryStrategy for BaselineEntry {
    fn id(&self) -> &'static str {
        self.policy.as_str()
    }
    fn decide(&self, ctx: &StrategyContext<'_>) -> StrategyDecision {
        match may_enter(
            self.policy,
            ctx.candidate,
            ctx.security,
            ctx.first_eligible_at,
            ctx.now,
            ctx.token,
            ctx.seed,
        ) {
            Ok(true) => StrategyDecision {
                enter: true,
                reason: "BASELINE",
            },
            Ok(false) => StrategyDecision {
                enter: false,
                reason: "BASELINE_SKIP",
            },
            Err(r) => StrategyDecision {
                enter: false,
                reason: r,
            },
        }
    }
}

pub struct RuleEntry {
    pub id: &'static str,
    pub thr: StrategyThresholds,
}

impl EntryStrategy for RuleEntry {
    fn id(&self) -> &'static str {
        self.id
    }
    fn decide(&self, ctx: &StrategyContext<'_>) -> StrategyDecision {
        if let Err(r) = gate_security_eligible(ctx) {
            return StrategyDecision {
                enter: false,
                reason: r,
            };
        }
        let Some(f) = ctx.features else {
            return StrategyDecision {
                enter: false,
                reason: "DATA_INCOMPLETE",
            };
        };
        if f.token_age_ms < self.thr.min_age_ms {
            return StrategyDecision {
                enter: false,
                reason: "MIN_AGE",
            };
        }
        let s = &f.shared;
        if self.id == "S1_BUYER_GROWTH"
            || self.id == "S2_FLOW_CONFIRMATION"
            || self.id == "S6_HYBRID"
        {
            match opt_i(&s.unique_buyer_acceleration_15s) {
                Some(a) if a >= self.thr.min_buyer_accel_15s => {}
                Some(_) => {
                    return StrategyDecision {
                        enter: false,
                        reason: "STRATEGY_FILTER",
                    };
                }
                None => {
                    return StrategyDecision {
                        enter: false,
                        reason: "DATA_INCOMPLETE",
                    };
                }
            }
            match opt_i(&s.unique_buyer_velocity_15s) {
                Some(v) if v >= self.thr.min_buyer_velocity_15s => {}
                _ => {
                    return StrategyDecision {
                        enter: false,
                        reason: "STRATEGY_FILTER",
                    };
                }
            }
        }
        if self.id == "S2_FLOW_CONFIRMATION" || self.id == "S6_HYBRID" {
            if self.thr.min_net_flow_non_negative && net_negative(&s.net_quote_flow_total) {
                return StrategyDecision {
                    enter: false,
                    reason: "STRATEGY_FILTER",
                };
            }
            if s.trade_count_imbalance < 0 {
                return StrategyDecision {
                    enter: false,
                    reason: "STRATEGY_FILTER",
                };
            }
        }
        if self.id == "S3_BROAD_PARTICIPATION" || self.id == "S6_HYBRID" {
            if s.unique_buyers_total < self.thr.min_unique_buyers {
                return StrategyDecision {
                    enter: false,
                    reason: "STRATEGY_FILTER",
                };
            }
            if let (Some(max_r), Some(r)) = (
                self.thr.max_repeat_buyer_ratio_bps,
                s.repeat_buyer_ratio_bps,
            ) {
                if r > max_r {
                    return StrategyDecision {
                        enter: false,
                        reason: "STRATEGY_FILTER",
                    };
                }
            }
            if let (Some(min_n), Some(n)) = (
                self.thr.min_new_buyer_ratio_30s_bps,
                s.new_buyer_ratio_30s_bps,
            ) {
                if n < min_n {
                    return StrategyDecision {
                        enter: false,
                        reason: "STRATEGY_FILTER",
                    };
                }
            }
        }
        if (self.id == "S4_CREATOR_FILTERED" || self.id == "S6_HYBRID")
            && self.thr.reject_creator_sold
            && s.creator_has_sold
        {
            return StrategyDecision {
                enter: false,
                reason: "STRATEGY_FILTER",
            };
        }
        if self.id == "S5_CURVE_CONFIRMATION" || self.id == "S6_HYBRID" {
            if let Some(p) = curve_bps(f) {
                if let Some(lo) = self.thr.min_curve_progress_bps {
                    if p < lo {
                        return StrategyDecision {
                            enter: false,
                            reason: "STRATEGY_FILTER",
                        };
                    }
                }
                if let Some(hi) = self.thr.max_curve_progress_bps {
                    if p > hi {
                        return StrategyDecision {
                            enter: false,
                            reason: "STRATEGY_FILTER",
                        };
                    }
                }
            } else if self.id == "S5_CURVE_CONFIRMATION" {
                return StrategyDecision {
                    enter: false,
                    reason: "DATA_INCOMPLETE",
                };
            }
            if s.unique_buyers_total < self.thr.min_unique_buyers {
                return StrategyDecision {
                    enter: false,
                    reason: "STRATEGY_FILTER",
                };
            }
        }
        StrategyDecision {
            enter: true,
            reason: "SIGNAL",
        }
    }
}

pub fn family(id: &str, thr: StrategyThresholds) -> Box<dyn EntryStrategy> {
    match id {
        "S0_BASELINE" | "E1_FIRST_ELIGIBLE" => Box::new(BaselineEntry {
            policy: EntryPolicyId::FirstEligible,
        }),
        "E2_ELIGIBLE_DELAY_30S" => Box::new(BaselineEntry {
            policy: EntryPolicyId::Delay30s,
        }),
        "E3_ELIGIBLE_DELAY_60S" => Box::new(BaselineEntry {
            policy: EntryPolicyId::Delay60s,
        }),
        "E4_ELIGIBLE_DELAY_120S" => Box::new(BaselineEntry {
            policy: EntryPolicyId::Delay120s,
        }),
        "E5_RANDOM_ELIGIBLE_CONTROL" => Box::new(BaselineEntry {
            policy: EntryPolicyId::RandomEligible,
        }),
        "S1_BUYER_GROWTH" => Box::new(RuleEntry {
            id: "S1_BUYER_GROWTH",
            thr,
        }),
        "S2_FLOW_CONFIRMATION" => Box::new(RuleEntry {
            id: "S2_FLOW_CONFIRMATION",
            thr,
        }),
        "S3_BROAD_PARTICIPATION" => Box::new(RuleEntry {
            id: "S3_BROAD_PARTICIPATION",
            thr,
        }),
        "S4_CREATOR_FILTERED" => {
            let mut t = thr;
            t.reject_creator_sold = true;
            Box::new(RuleEntry {
                id: "S4_CREATOR_FILTERED",
                thr: t,
            })
        }
        "S5_CURVE_CONFIRMATION" => Box::new(RuleEntry {
            id: "S5_CURVE_CONFIRMATION",
            thr,
        }),
        "S6_HYBRID" => {
            let mut t = thr;
            t.reject_creator_sold = true;
            Box::new(RuleEntry {
                id: "S6_HYBRID",
                thr: t,
            })
        }
        _ => Box::new(BaselineEntry {
            policy: EntryPolicyId::FirstEligible,
        }),
    }
}

pub fn all_families() -> [&'static str; 7] {
    [
        "S0_BASELINE",
        "S1_BUYER_GROWTH",
        "S2_FLOW_CONFIRMATION",
        "S3_BROAD_PARTICIPATION",
        "S4_CREATOR_FILTERED",
        "S5_CURVE_CONFIRMATION",
        "S6_HYBRID",
    ]
}
