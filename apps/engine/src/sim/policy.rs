//! Baseline research entry/exit policies. Not optimized against PnL.

use chrono::{DateTime, Utc};

use crate::candidate::CandidateState;
use crate::security::assessment::SecurityVerdict;
use crate::state::TokenStateSnapshot;

use super::models::deterministic_hit;
use super::position::{return_bps, ExitPolicy, SimulatedPosition};
use super::types::ExitReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryPolicyId {
    FirstEligible,
    Delay30s,
    Delay60s,
    Delay120s,
    RandomEligible,
}

impl EntryPolicyId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirstEligible => "E1_FIRST_ELIGIBLE",
            Self::Delay30s => "E2_ELIGIBLE_DELAY_30S",
            Self::Delay60s => "E3_ELIGIBLE_DELAY_60S",
            Self::Delay120s => "E4_ELIGIBLE_DELAY_120S",
            Self::RandomEligible => "E5_RANDOM_ELIGIBLE_CONTROL",
        }
    }

    pub fn delay_ms(self) -> i64 {
        match self {
            Self::FirstEligible | Self::RandomEligible => 0,
            Self::Delay30s => 30_000,
            Self::Delay60s => 60_000,
            Self::Delay120s => 120_000,
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "E1_FIRST_ELIGIBLE" | "e1" => Self::FirstEligible,
            "E2_ELIGIBLE_DELAY_30S" | "e2" => Self::Delay30s,
            "E3_ELIGIBLE_DELAY_60S" | "e3" => Self::Delay60s,
            "E4_ELIGIBLE_DELAY_120S" | "e4" => Self::Delay120s,
            "E5_RANDOM_ELIGIBLE_CONTROL" | "e5" => Self::RandomEligible,
            _ => return None,
        })
    }
}

pub fn all_entry_policies() -> [EntryPolicyId; 5] {
    [
        EntryPolicyId::FirstEligible,
        EntryPolicyId::Delay30s,
        EntryPolicyId::Delay60s,
        EntryPolicyId::Delay120s,
        EntryPolicyId::RandomEligible,
    ]
}

/// Security REJECT never enters. UNKNOWN is not research-valid.
pub fn may_enter(
    policy: EntryPolicyId,
    candidate: CandidateState,
    security: Option<SecurityVerdict>,
    first_eligible_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    token: &str,
    seed: u64,
) -> Result<bool, &'static str> {
    match security {
        Some(SecurityVerdict::Reject) => return Err("SECURITY_REJECT"),
        Some(SecurityVerdict::Unknown) | None => return Err("SECURITY_UNKNOWN"),
        Some(SecurityVerdict::Pass) | Some(SecurityVerdict::Warn) => {}
    }
    if candidate != CandidateState::Eligible {
        return Err("NOT_ELIGIBLE");
    }
    let Some(t0) = first_eligible_at else {
        return Err("NO_ELIGIBLE_TIME");
    };
    let due = t0 + chrono::Duration::milliseconds(policy.delay_ms());
    if now < due {
        return Err("DELAY_NOT_REACHED");
    }
    if policy == EntryPolicyId::RandomEligible
        && !deterministic_hit(seed, token, t0.timestamp_millis(), 0, 5_000)
    {
        return Err("RANDOM_CONTROL_SKIP");
    }
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct TimeExit {
    pub hold_ms: i64,
    pub id: &'static str,
}

impl ExitPolicy for TimeExit {
    fn id(&self) -> &'static str {
        self.id
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        snap: &TokenStateSnapshot,
        _mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        _flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let held = snap
            .snapshot_time
            .signed_duration_since(pos.opened_at)
            .num_milliseconds();
        if held >= self.hold_ms {
            Some((
                ExitReason::TimeStop,
                pos.remaining_token_amount.clone(),
                true,
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct FixedTpSl {
    pub tp_bps: i64,
    pub sl_bps: i64,
}

impl ExitPolicy for FixedTpSl {
    fn id(&self) -> &'static str {
        "X4_FIXED_TP_SL"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        _snap: &TokenStateSnapshot,
        mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        _flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let mark = mark?;
        let r = return_bps(&pos.quote_cost, mark)?;
        if r >= self.tp_bps {
            Some((
                ExitReason::TakeProfit,
                pos.remaining_token_amount.clone(),
                true,
            ))
        } else if r <= -self.sl_bps {
            Some((ExitReason::Stop, pos.remaining_token_amount.clone(), true))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrailingExit {
    pub trail_bps: u32,
}

impl ExitPolicy for TrailingExit {
    fn id(&self) -> &'static str {
        "X5_TRAILING"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        _snap: &TokenStateSnapshot,
        mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        _flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let mark = mark?;
        let peak = return_bps(&pos.quote_cost, &pos.highest_mark_quote)?;
        let now = return_bps(&pos.quote_cost, mark)?;
        if peak > 0 && peak - now >= i64::from(self.trail_bps) {
            Some((ExitReason::Trail, pos.remaining_token_amount.clone(), true))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct PartialRunner {
    pub stages: Vec<(i64, u32)>,
    pub trail_bps: u32,
}

impl Default for PartialRunner {
    fn default() -> Self {
        Self {
            stages: vec![(4_000, 2_000), (8_000, 2_000), (20_000, 2_000)],
            trail_bps: 3_000,
        }
    }
}

impl ExitPolicy for PartialRunner {
    fn id(&self) -> &'static str {
        "X6_PARTIAL_RUNNER"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        _snap: &TokenStateSnapshot,
        mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        _flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let mark = mark?;
        let r = return_bps(&pos.quote_cost, mark)?;
        let partials = pos
            .events
            .iter()
            .filter(|e| e.kind == super::types::PositionEventKind::PartialExit)
            .count();
        if let Some(&(need, sell_bps)) = self.stages.get(partials) {
            if r >= need {
                let rem = crate::state::amt::parse_u256(&pos.remaining_token_amount);
                let sell = rem.saturating_mul(alloy_primitives::U256::from(sell_bps))
                    / alloy_primitives::U256::from(10_000u64);
                if sell.is_zero() {
                    return None;
                }
                return Some((
                    ExitReason::PartialScale,
                    crate::state::amt::u256_dec(sell),
                    false,
                ));
            }
        }
        let peak = return_bps(&pos.quote_cost, &pos.highest_mark_quote)?;
        if partials >= self.stages.len() && peak > 0 && peak - r >= i64::from(self.trail_bps) {
            Some((ExitReason::Trail, pos.remaining_token_amount.clone(), true))
        } else {
            None
        }
    }
}

pub fn exit_policy(id: &str) -> Box<dyn ExitPolicy> {
    match id {
        "X1_TIME_2M" | "x1" => Box::new(TimeExit {
            hold_ms: 120_000,
            id: "X1_TIME_2M",
        }),
        "X2_TIME_5M" | "x2" => Box::new(TimeExit {
            hold_ms: 300_000,
            id: "X2_TIME_5M",
        }),
        "X3_TIME_15M" | "x3" => Box::new(TimeExit {
            hold_ms: 900_000,
            id: "X3_TIME_15M",
        }),
        "X4_FIXED_TP_SL" | "x4" => Box::new(FixedTpSl {
            tp_bps: 10_000,
            sl_bps: 5_000,
        }),
        "X5_TRAILING" | "x5" => Box::new(TrailingExit { trail_bps: 3_000 }),
        "X6_PARTIAL_RUNNER" | "x6" => Box::new(PartialRunner::default()),
        "X7_FLOW_DECAY" | "x7" => Box::new(FlowDecayExit),
        "X8_CREATOR_SELL" | "x8" => Box::new(CreatorSellExit),
        "X9_DYNAMIC_RUNNER" | "x9" => Box::new(DynamicRunner {
            inner: PartialRunner::default(),
            time_cap_ms: 900_000,
        }),
        _ => Box::new(TimeExit {
            hold_ms: 300_000,
            id: "X2_TIME_5M",
        }),
    }
}

pub fn all_exit_ids() -> [&'static str; 9] {
    [
        "X1_TIME_2M",
        "X2_TIME_5M",
        "X3_TIME_15M",
        "X4_FIXED_TP_SL",
        "X5_TRAILING",
        "X6_PARTIAL_RUNNER",
        "X7_FLOW_DECAY",
        "X8_CREATOR_SELL",
        "X9_DYNAMIC_RUNNER",
    ]
}

pub struct FlowDecayExit;

impl ExitPolicy for FlowDecayExit {
    fn id(&self) -> &'static str {
        "X7_FLOW_DECAY"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        _snap: &TokenStateSnapshot,
        _mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let f = flow?;
        let buyer_collapse = f.unique_buyer_accel_15s.is_some_and(|a| a <= -2);
        let seller_up = f.unique_seller_accel_15s.is_some_and(|a| a >= 2);
        if buyer_collapse || (seller_up && f.net_flow_negative) {
            Some((
                ExitReason::MomentumDecay,
                pos.remaining_token_amount.clone(),
                true,
            ))
        } else {
            None
        }
    }
}

pub struct CreatorSellExit;

impl ExitPolicy for CreatorSellExit {
    fn id(&self) -> &'static str {
        "X8_CREATOR_SELL"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        _snap: &TokenStateSnapshot,
        _mark: Option<&str>,
        _sec: Option<SecurityVerdict>,
        flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        let sells = flow.map(|f| f.creator_sell_count).unwrap_or(0);
        if sells > pos.creator_sell_count_at_entry {
            Some((
                ExitReason::CreatorSell,
                pos.remaining_token_amount.clone(),
                true,
            ))
        } else {
            None
        }
    }
}

pub struct DynamicRunner {
    pub inner: PartialRunner,
    pub time_cap_ms: i64,
}

impl ExitPolicy for DynamicRunner {
    fn id(&self) -> &'static str {
        "X9_DYNAMIC_RUNNER"
    }
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        snap: &TokenStateSnapshot,
        mark: Option<&str>,
        sec: Option<SecurityVerdict>,
        flow: Option<&super::position::FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        if let Some(x) = FlowDecayExit.on_mark(pos, snap, mark, sec, flow) {
            return Some(x);
        }
        if let Some(x) = CreatorSellExit.on_mark(pos, snap, mark, sec, flow) {
            return Some(x);
        }
        let held = snap
            .snapshot_time
            .signed_duration_since(pos.opened_at)
            .num_milliseconds();
        if held >= self.time_cap_ms {
            return Some((
                ExitReason::TimeStop,
                pos.remaining_token_amount.clone(),
                true,
            ));
        }
        self.inner.on_mark(pos, snap, mark, sec, flow)
    }
}
