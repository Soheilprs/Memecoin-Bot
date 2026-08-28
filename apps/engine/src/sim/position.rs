//! Simulated positions and PositionManager. Exit policy is injected; MFE never feeds entry.
#![allow(clippy::too_many_arguments)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad};
use crate::security::assessment::SecurityVerdict;
use crate::state::amt::{add_raw, parse_u256, sub_sat_raw, u256_dec};
use crate::state::TokenStateSnapshot;

use super::exec::ExecutionResult;
use super::impact::{mark_exit_quote, spot_price_1e18};
use super::models::FeeModel;
use super::types::{ExitReason, PositionEventKind, PositionStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedPosition {
    pub id: i64,
    pub simulation_run_id: Option<i64>,
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub strategy_policy_id: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub entry_execution_id: Option<i64>,
    pub initial_token_amount: String,
    pub remaining_token_amount: String,
    pub quote_cost: String,
    pub realized_quote: String,
    pub realized_pnl_quote: String,
    pub status: PositionStatus,
    pub highest_mark_quote: String,
    pub lowest_mark_quote: String,
    pub mfe_quote: String,
    pub mae_quote: String,
    pub mfe_bps: Option<i64>,
    pub mae_bps: Option<i64>,
    pub capture_ratio_bps: Option<u32>,
    pub token_max_return_bps: Option<i64>,
    pub entry_price_1e18: String,
    pub entry_feature_vector_id: Option<i64>,
    pub entry_security_assessment_id: Option<i64>,
    pub entry_research_valid: bool,
    #[serde(default)]
    pub creator_sell_count_at_entry: u64,
    pub events: Vec<PositionEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionEvent {
    pub kind: PositionEventKind,
    pub at: DateTime<Utc>,
    pub token_delta: String,
    pub quote_delta: String,
    pub remaining_token: String,
    pub reason: Option<String>,
    pub execution_status: Option<String>,
}

impl SimulatedPosition {
    pub fn open(
        id: i64,
        chain: Chain,
        token: String,
        launchpad: Launchpad,
        policy: String,
        fill: &ExecutionResult,
        feature_id: Option<i64>,
        sec_id: Option<i64>,
    ) -> Self {
        let tok = fill.filled_token.clone();
        let q = fill.filled_quote.clone();
        Self {
            id,
            simulation_run_id: None,
            chain,
            token,
            launchpad,
            strategy_policy_id: policy,
            opened_at: fill
                .actual_simulated_fill_time
                .unwrap_or(fill.decision_time),
            closed_at: None,
            entry_execution_id: fill.id,
            initial_token_amount: tok.clone(),
            remaining_token_amount: tok,
            quote_cost: q.clone(),
            realized_quote: "0".into(),
            realized_pnl_quote: "0".into(),
            status: PositionStatus::Open,
            highest_mark_quote: q.clone(),
            lowest_mark_quote: q.clone(),
            mfe_quote: q.clone(),
            mae_quote: q.clone(),
            mfe_bps: Some(0),
            mae_bps: Some(0),
            capture_ratio_bps: None,
            token_max_return_bps: Some(0),
            entry_price_1e18: fill.effective_fill_price_1e18.clone(),
            entry_feature_vector_id: feature_id,
            entry_security_assessment_id: sec_id,
            entry_research_valid: fill.research_valid,
            creator_sell_count_at_entry: 0,
            events: vec![PositionEvent {
                kind: PositionEventKind::PositionOpened,
                at: fill
                    .actual_simulated_fill_time
                    .unwrap_or(fill.decision_time),
                token_delta: fill.filled_token.clone(),
                quote_delta: fill.filled_quote.clone(),
                remaining_token: fill.filled_token.clone(),
                reason: None,
                execution_status: Some(fill.status.as_str().into()),
            }],
        }
    }

    /// Apply a persisted fill. CLOSED only when remaining inventory is exactly zero.
    /// `full` is the policy's intended close; it must not synthesize a close if tokens remain.
    pub fn apply_exit(&mut self, fill: &ExecutionResult, reason: ExitReason, _full: bool) {
        if !fill.status.is_fill() {
            self.events.push(PositionEvent {
                kind: if reason.is_emergency() {
                    PositionEventKind::EmergencySignal
                } else {
                    PositionEventKind::ExitAttemptFailed
                },
                at: fill.decision_time,
                token_delta: "0".into(),
                quote_delta: "0".into(),
                remaining_token: self.remaining_token_amount.clone(),
                reason: Some(format!("{}_{}", reason.as_str(), fill.status.as_str())),
                execution_status: Some(fill.status.as_str().into()),
            });
            return;
        }
        let rem = parse_u256(&self.remaining_token_amount);
        let mut sold = parse_u256(&fill.filled_token);
        if sold > rem {
            sold = rem;
        }
        if sold.is_zero() {
            self.events.push(PositionEvent {
                kind: PositionEventKind::ExitAttemptFailed,
                at: fill.decision_time,
                token_delta: "0".into(),
                quote_delta: "0".into(),
                remaining_token: self.remaining_token_amount.clone(),
                reason: Some(format!("{}_ZERO_SOLD", reason.as_str())),
                execution_status: Some(fill.status.as_str().into()),
            });
            return;
        }
        let new_rem = rem - sold;
        let init = parse_u256(&self.initial_token_amount);
        let cost = parse_u256(&self.quote_cost);
        let sold_cost = if init.is_zero() {
            alloy_primitives::U256::ZERO
        } else {
            cost.saturating_mul(sold) / init
        };
        let recv = parse_u256(&fill.filled_quote);
        let pnl = if recv >= sold_cost {
            format!("+{}", u256_dec(recv - sold_cost))
        } else {
            format!("-{}", u256_dec(sold_cost - recv))
        };
        self.remaining_token_amount = u256_dec(new_rem);
        self.realized_quote = add_raw(&self.realized_quote, &fill.filled_quote);
        self.realized_pnl_quote = signed_add(&self.realized_pnl_quote, &pnl);
        let kind = if new_rem.is_zero() {
            self.status = PositionStatus::Closed;
            self.closed_at = fill.actual_simulated_fill_time.or(Some(fill.decision_time));
            self.finalize_capture();
            PositionEventKind::PositionClosed
        } else {
            PositionEventKind::PartialExit
        };
        self.events.push(PositionEvent {
            kind,
            at: fill
                .actual_simulated_fill_time
                .unwrap_or(fill.decision_time),
            token_delta: format!("-{}", u256_dec(sold)),
            quote_delta: fill.filled_quote.clone(),
            remaining_token: self.remaining_token_amount.clone(),
            reason: Some(reason.as_str().into()),
            execution_status: Some(fill.status.as_str().into()),
        });
    }

    pub fn is_closed_flat(&self) -> bool {
        self.status == PositionStatus::Closed && parse_u256(&self.remaining_token_amount).is_zero()
    }

    pub fn mark(&mut self, snap: &TokenStateSnapshot, fees: &FeeModel) {
        if self.status != PositionStatus::Open {
            return;
        }
        if let Some(px) = spot_price_1e18(snap) {
            if let Some(ret) = return_bps(&self.entry_price_1e18, &px) {
                let cur = self.token_max_return_bps.unwrap_or(i64::MIN);
                if ret > cur {
                    self.token_max_return_bps = Some(ret);
                }
            }
        }
        let Some(mark) = mark_exit_quote(snap, &self.remaining_token_amount, fees) else {
            return;
        };
        if parse_u256(&mark) > parse_u256(&self.highest_mark_quote) {
            self.highest_mark_quote = mark.clone();
            self.mfe_quote = mark.clone();
        }
        if parse_u256(&self.lowest_mark_quote).is_zero()
            || parse_u256(&mark) < parse_u256(&self.lowest_mark_quote)
        {
            self.lowest_mark_quote = mark.clone();
            self.mae_quote = mark.clone();
        }
        self.mfe_bps = return_bps(&self.quote_cost, &self.highest_mark_quote);
        self.mae_bps = return_bps(&self.quote_cost, &self.lowest_mark_quote);
    }

    /// Bounded smoke window ended. Do not force-sell at last mark.
    pub fn end_session_open(&mut self, at: DateTime<Utc>) {
        if self.status != PositionStatus::Open {
            return;
        }
        self.status = PositionStatus::SessionEndedOpen;
        self.events.push(PositionEvent {
            kind: PositionEventKind::ForcedEndOfData,
            at,
            token_delta: "0".into(),
            quote_delta: "0".into(),
            remaining_token: self.remaining_token_amount.clone(),
            reason: Some("SESSION_ENDED_OPEN".into()),
            execution_status: None,
        });
    }

    pub fn force_end(&mut self, at: DateTime<Utc>, realizable: bool) {
        if self.status != PositionStatus::Open {
            return;
        }
        if realizable {
            self.status = PositionStatus::ForcedEndOfData;
        } else {
            self.status = PositionStatus::Unrealizable;
            crate::metrics::DiscoveryMetrics::sim_unsellable();
        }
        self.closed_at = Some(at);
        self.events.push(PositionEvent {
            kind: PositionEventKind::ForcedEndOfData,
            at,
            token_delta: "0".into(),
            quote_delta: "0".into(),
            remaining_token: self.remaining_token_amount.clone(),
            reason: Some(if realizable {
                "END_OF_DATA".into()
            } else {
                "UNREALIZABLE_POSITION".into()
            }),
            execution_status: None,
        });
        self.finalize_capture();
    }

    fn finalize_capture(&mut self) {
        self.capture_ratio_bps =
            capture_ratio_bps(&self.quote_cost, &self.realized_quote, &self.mfe_quote);
    }
}

pub fn return_bps(cost: &str, value: &str) -> Option<i64> {
    let c = parse_u256(cost);
    if c.is_zero() {
        return None;
    }
    let v = parse_u256(value);
    if v >= c {
        let bps = (v - c).saturating_mul(alloy_primitives::U256::from(10_000u64)) / c;
        Some(i64::try_from(bps).unwrap_or(i64::MAX))
    } else {
        let bps = (c - v).saturating_mul(alloy_primitives::U256::from(10_000u64)) / c;
        Some(-i64::try_from(bps).unwrap_or(i64::MAX))
    }
}

/// realized_pnl / MFE_pnl when MFE_pnl > 0, else None. Never uses f64.
pub fn capture_ratio_bps(cost: &str, realized: &str, mfe: &str) -> Option<u32> {
    let c = parse_u256(cost);
    let r = parse_u256(realized);
    let m = parse_u256(mfe);
    if m <= c {
        return None;
    }
    let mfe_pnl = m - c;
    let real_pnl = if r >= c {
        r - c
    } else {
        alloy_primitives::U256::ZERO
    };
    Some(
        u32::try_from(real_pnl.saturating_mul(alloy_primitives::U256::from(10_000u64)) / mfe_pnl)
            .unwrap_or(u32::MAX),
    )
}

fn signed_add(a: &str, b: &str) -> String {
    fn parse_signed(s: &str) -> (bool, alloy_primitives::U256) {
        let t = s.trim();
        if let Some(x) = t.strip_prefix('-') {
            (true, parse_u256(x))
        } else if let Some(x) = t.strip_prefix('+') {
            (false, parse_u256(x))
        } else {
            (false, parse_u256(t))
        }
    }
    let (an, av) = parse_signed(a);
    let (bn, bv) = parse_signed(b);
    match (an, bn) {
        (false, false) => add_raw(&u256_dec(av), &u256_dec(bv)),
        (true, true) => format!("-{}", add_raw(&u256_dec(av), &u256_dec(bv))),
        (false, true) if av >= bv => sub_sat_raw(&u256_dec(av), &u256_dec(bv)),
        (false, true) => format!("-{}", sub_sat_raw(&u256_dec(bv), &u256_dec(av))),
        (true, false) if bv >= av => sub_sat_raw(&u256_dec(bv), &u256_dec(av)),
        (true, false) => format!("-{}", sub_sat_raw(&u256_dec(av), &u256_dec(bv))),
    }
}

pub struct PositionManager<'a> {
    pub policy: &'a dyn ExitPolicy,
    pub fees: &'a FeeModel,
}

#[derive(Debug, Clone, Default)]
pub struct FlowSignal {
    pub unique_buyer_accel_15s: Option<i64>,
    pub unique_seller_accel_15s: Option<i64>,
    pub net_flow_negative: bool,
    pub creator_sell_count: u64,
}

pub trait ExitPolicy: Send + Sync {
    fn id(&self) -> &'static str;
    fn on_mark(
        &self,
        pos: &SimulatedPosition,
        snap: &TokenStateSnapshot,
        mark_quote: Option<&str>,
        security: Option<SecurityVerdict>,
        flow: Option<&FlowSignal>,
    ) -> Option<(ExitReason, String, bool)>;
}

impl PositionManager<'_> {
    pub fn evaluate(
        &self,
        pos: &SimulatedPosition,
        snap: &TokenStateSnapshot,
        security: Option<SecurityVerdict>,
        flow: Option<&FlowSignal>,
    ) -> Option<(ExitReason, String, bool)> {
        if pos.status != PositionStatus::Open {
            return None;
        }
        if matches!(security, Some(SecurityVerdict::Reject)) {
            return Some((
                ExitReason::SecurityEmergency,
                pos.remaining_token_amount.clone(),
                true,
            ));
        }
        let mark = mark_exit_quote(snap, &pos.remaining_token_amount, self.fees);
        self.policy
            .on_mark(pos, snap, mark.as_deref(), security, flow)
    }
}
