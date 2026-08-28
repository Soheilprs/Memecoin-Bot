//! OutcomeEngine. May inspect the future. Must not be imported by FeatureEngine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::candidate::CandidateState;
use crate::domain::{Chain, Launchpad, QualityStatus};
use crate::security::assessment::SecurityVerdict;
use crate::state::amt::parse_u256;
use crate::state::TokenStateSnapshot;

use super::harness::SimulationReport;
use super::impact::spot_price_1e18;
use super::position::{return_bps, SimulatedPosition};
use super::types::{ExecutionStatus, OUTCOME_MODEL_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MissReason {
    SecurityReject,
    SecurityUnknown,
    DataIncomplete,
    NeverConfirming,
    ExpiredTooEarly,
    StrategyFilter,
    EntryFailed,
    LiquidityTooLow,
    EnteredExitedTooEarly,
    EnteredCaptured,
    NeverEligible,
    RandomSkip,
}

impl MissReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SecurityReject => "SECURITY_REJECT",
            Self::SecurityUnknown => "SECURITY_UNKNOWN",
            Self::DataIncomplete => "DATA_INCOMPLETE",
            Self::NeverConfirming => "NEVER_CONFIRMING",
            Self::ExpiredTooEarly => "EXPIRED_TOO_EARLY",
            Self::StrategyFilter => "STRATEGY_FILTER",
            Self::EntryFailed => "ENTRY_FAILED",
            Self::LiquidityTooLow => "LIQUIDITY_TOO_LOW",
            Self::EnteredExitedTooEarly => "EXITED_TOO_EARLY",
            Self::EnteredCaptured => "ENTERED_CAPTURED",
            Self::NeverEligible => "NEVER_ELIGIBLE",
            Self::RandomSkip => "RANDOM_CONTROL_SKIP",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenOutcome {
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub reference_time: DateTime<Utc>,
    pub reference_snapshot_id: Option<i64>,
    pub horizon_ms: i64,
    pub reference_price_1e18: String,
    pub final_price_1e18: String,
    pub max_price_1e18: String,
    pub min_price_1e18: String,
    pub final_return_bps: Option<i64>,
    pub max_return_bps: Option<i64>,
    pub max_drawdown_bps: Option<i64>,
    pub reached_2x: bool,
    pub reached_5x: bool,
    pub reached_10x: bool,
    pub reached_20x: bool,
    pub time_to_2x_ms: Option<i64>,
    pub time_to_5x_ms: Option<i64>,
    pub time_to_10x_ms: Option<i64>,
    pub time_to_20x_ms: Option<i64>,
    pub outcome_quality: QualityStatus,
    pub outcome_model_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissedWinner {
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub max_return_bps: Option<i64>,
    pub time_to_5x_ms: Option<i64>,
    pub time_to_10x_ms: Option<i64>,
    pub security: Option<SecurityVerdict>,
    pub highest_candidate: CandidateState,
    pub entered: bool,
    pub realized_return_bps: Option<i64>,
    pub mfe_bps: Option<i64>,
    pub capture_ratio_bps: Option<u32>,
    pub miss_reason: MissReason,
}

pub struct OutcomeEngine;

impl OutcomeEngine {
    /// Labels after `reference`. Future prices are allowed here only.
    pub fn token_outcome(
        snaps: &[TokenStateSnapshot],
        chain: Chain,
        token: &str,
        reference: DateTime<Utc>,
        horizon_ms: i64,
    ) -> Option<TokenOutcome> {
        let series: Vec<_> = snaps
            .iter()
            .filter(|s| s.chain == chain && s.token_address == token)
            .collect();
        if series.is_empty() {
            return None;
        }
        let launchpad = series[0].launchpad;
        let ref_snap = series
            .iter()
            .filter(|s| s.snapshot_time <= reference)
            .max_by_key(|s| s.snapshot_time)
            .copied();
        let r = ref_snap?;
        let Some(ref_px) = spot_price_1e18(r) else {
            // still emit a dead/zero outcome so rejected/dead tokens are counted
            return Some(empty_outcome(r, reference, horizon_ms));
        };
        let end = reference + chrono::Duration::milliseconds(horizon_ms);
        let mut max_px = parse_u256(&ref_px);
        let mut min_px = max_px;
        let mut final_px = max_px;
        let mut t2 = None;
        let mut t5 = None;
        let mut t10 = None;
        let mut t20 = None;
        let rp = parse_u256(&ref_px);
        for s in series
            .iter()
            .filter(|s| s.snapshot_time > reference && s.snapshot_time <= end)
        {
            let Some(pxs) = spot_price_1e18(s) else {
                continue;
            };
            let px = parse_u256(&pxs);
            if px > max_px {
                max_px = px;
            }
            if px < min_px {
                min_px = px;
            }
            final_px = px;
            if !rp.is_zero() {
                let elapsed = s
                    .snapshot_time
                    .signed_duration_since(reference)
                    .num_milliseconds();
                if t2.is_none() && px >= rp * alloy_primitives::U256::from(2u64) {
                    t2 = Some(elapsed);
                }
                if t5.is_none() && px >= rp * alloy_primitives::U256::from(5u64) {
                    t5 = Some(elapsed);
                }
                if t10.is_none() && px >= rp * alloy_primitives::U256::from(10u64) {
                    t10 = Some(elapsed);
                }
                if t20.is_none() && px >= rp * alloy_primitives::U256::from(20u64) {
                    t20 = Some(elapsed);
                }
            }
        }
        let max_ret = return_bps(
            &crate::state::amt::u256_dec(rp),
            &crate::state::amt::u256_dec(max_px),
        );
        let fin_ret = return_bps(
            &crate::state::amt::u256_dec(rp),
            &crate::state::amt::u256_dec(final_px),
        );
        let dd = return_bps(
            &crate::state::amt::u256_dec(rp),
            &crate::state::amt::u256_dec(min_px),
        );
        Some(TokenOutcome {
            chain,
            token: token.into(),
            launchpad,
            reference_time: reference,
            reference_snapshot_id: r.id,
            horizon_ms,
            reference_price_1e18: crate::state::amt::u256_dec(rp),
            final_price_1e18: crate::state::amt::u256_dec(final_px),
            max_price_1e18: crate::state::amt::u256_dec(max_px),
            min_price_1e18: crate::state::amt::u256_dec(min_px),
            final_return_bps: fin_ret,
            max_return_bps: max_ret,
            max_drawdown_bps: dd,
            reached_2x: t2.is_some()
                || max_px >= rp.saturating_mul(alloy_primitives::U256::from(2u64)),
            reached_5x: t5.is_some()
                || max_px >= rp.saturating_mul(alloy_primitives::U256::from(5u64)),
            reached_10x: t10.is_some()
                || max_px >= rp.saturating_mul(alloy_primitives::U256::from(10u64)),
            reached_20x: t20.is_some()
                || max_px >= rp.saturating_mul(alloy_primitives::U256::from(20u64)),
            time_to_2x_ms: t2,
            time_to_5x_ms: t5,
            time_to_10x_ms: t10,
            time_to_20x_ms: t20,
            outcome_quality: r.data_quality,
            outcome_model_version: OUTCOME_MODEL_VERSION.into(),
        })
    }

    pub fn outcomes_for_all(
        snaps: &[TokenStateSnapshot],
        reference_offset_ms: i64,
        horizon_ms: i64,
    ) -> Vec<TokenOutcome> {
        let mut keys = Vec::new();
        for s in snaps {
            let k = (s.chain, s.token_address.clone());
            if !keys
                .iter()
                .any(|x: &(Chain, String)| x.0 == k.0 && x.1 == k.1)
            {
                keys.push(k);
            }
        }
        let mut out = Vec::new();
        for (chain, token) in keys {
            let first = snaps
                .iter()
                .find(|s| s.chain == chain && s.token_address == token)
                .map(|s| s.snapshot_time);
            let Some(t0) = first else {
                continue;
            };
            let reference = t0 + chrono::Duration::milliseconds(reference_offset_ms);
            if let Some(o) = Self::token_outcome(snaps, chain, &token, reference, horizon_ms) {
                out.push(o);
            }
        }
        out
    }

    pub fn missed_winners(
        outcomes: &[TokenOutcome],
        report: &SimulationReport,
        security: &[(DateTime<Utc>, Chain, String, SecurityVerdict)],
        candidate: &[(DateTime<Utc>, Chain, String, CandidateState)],
        min_max_return_bps: i64,
    ) -> Vec<MissedWinner> {
        let mut rows = Vec::new();
        for o in outcomes
            .iter()
            .filter(|o| o.max_return_bps.unwrap_or(0) >= min_max_return_bps)
        {
            let pos = report
                .positions
                .iter()
                .find(|p| p.chain == o.chain && p.token == o.token);
            let order = report.orders.iter().rfind(|x| {
                x.chain == o.chain && x.token == o.token && x.side == super::types::OrderSide::Buy
            });
            let sec = security
                .iter()
                .filter(|s| s.1 == o.chain && s.2 == o.token)
                .max_by_key(|s| s.0)
                .map(|s| s.3);
            let hi = candidate
                .iter()
                .filter(|s| s.1 == o.chain && s.2 == o.token)
                .map(|s| s.3)
                .max_by_key(|c| rank_cand(*c))
                .unwrap_or(CandidateState::Discovered);
            let reason = classify_miss(pos, order.map(|o| o.status), sec, hi, o);
            if matches!(
                reason,
                MissReason::NeverEligible
                    | MissReason::SecurityReject
                    | MissReason::EntryFailed
                    | MissReason::EnteredExitedTooEarly
            ) {
                crate::metrics::DiscoveryMetrics::sim_missed_winner();
            }
            rows.push(MissedWinner {
                chain: o.chain,
                token: o.token.clone(),
                launchpad: o.launchpad,
                max_return_bps: o.max_return_bps,
                time_to_5x_ms: o.time_to_5x_ms,
                time_to_10x_ms: o.time_to_10x_ms,
                security: sec,
                highest_candidate: hi,
                entered: pos.is_some(),
                realized_return_bps: pos.and_then(|p| return_bps(&p.quote_cost, &p.realized_quote)),
                mfe_bps: pos.and_then(|p| p.mfe_bps),
                capture_ratio_bps: pos.and_then(|p| p.capture_ratio_bps),
                miss_reason: reason,
            });
        }
        rows
    }
}

fn empty_outcome(
    r: &TokenStateSnapshot,
    reference: DateTime<Utc>,
    horizon_ms: i64,
) -> TokenOutcome {
    TokenOutcome {
        chain: r.chain,
        token: r.token_address.clone(),
        launchpad: r.launchpad,
        reference_time: reference,
        reference_snapshot_id: r.id,
        horizon_ms,
        reference_price_1e18: "0".into(),
        final_price_1e18: "0".into(),
        max_price_1e18: "0".into(),
        min_price_1e18: "0".into(),
        final_return_bps: None,
        max_return_bps: None,
        max_drawdown_bps: None,
        reached_2x: false,
        reached_5x: false,
        reached_10x: false,
        reached_20x: false,
        time_to_2x_ms: None,
        time_to_5x_ms: None,
        time_to_10x_ms: None,
        time_to_20x_ms: None,
        outcome_quality: r.data_quality,
        outcome_model_version: OUTCOME_MODEL_VERSION.into(),
    }
}

fn rank_cand(c: CandidateState) -> u8 {
    match c {
        CandidateState::Discovered => 0,
        CandidateState::SecurityPending => 1,
        CandidateState::DataIncomplete => 2,
        CandidateState::SecurityRejected => 3,
        CandidateState::Watching => 4,
        CandidateState::Confirming => 5,
        CandidateState::Eligible => 6,
        CandidateState::Expired => 2,
    }
}

fn classify_miss(
    pos: Option<&SimulatedPosition>,
    buy_status: Option<ExecutionStatus>,
    sec: Option<SecurityVerdict>,
    hi: CandidateState,
    outcome: &TokenOutcome,
) -> MissReason {
    if let Some(p) = pos {
        let cap = p.capture_ratio_bps.unwrap_or(0);
        if outcome.reached_10x && cap < 2_500 {
            return MissReason::EnteredExitedTooEarly;
        }
        return MissReason::EnteredCaptured;
    }
    if matches!(sec, Some(SecurityVerdict::Reject)) {
        return MissReason::SecurityReject;
    }
    if matches!(sec, Some(SecurityVerdict::Unknown) | None) {
        return MissReason::SecurityUnknown;
    }
    if hi != CandidateState::Eligible {
        return MissReason::NeverEligible;
    }
    match buy_status {
        Some(ExecutionStatus::Failed) => MissReason::EntryFailed,
        Some(ExecutionStatus::RejectedLiquidity | ExecutionStatus::UnavailableMarketState) => {
            MissReason::LiquidityTooLow
        }
        Some(_) => MissReason::EntryFailed,
        None => MissReason::StrategyFilter,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyPerformance {
    pub policy_id: String,
    pub n_orders: usize,
    pub filled_entries: usize,
    pub fill_rate_bps: u32,
    pub trades_closed: usize,
    pub forced_end: usize,
    pub net_pnl_quote: String,
    pub win_rate_bps: u32,
    pub expectancy_bps: Option<i64>,
    pub profit_factor_milli: Option<u32>,
    pub largest_winner_bps: Option<i64>,
    pub largest_loser_bps: Option<i64>,
    pub trades_gt_2x: usize,
    pub trades_gt_5x: usize,
    pub trades_gt_10x: usize,
    pub sample_insufficient: bool,
    pub research_valid: bool,
}

pub fn policy_performance(report: &SimulationReport) -> PolicyPerformance {
    let entries: Vec<_> = report
        .orders
        .iter()
        .filter(|o| o.side == super::types::OrderSide::Buy)
        .collect();
    let filled = entries.iter().filter(|o| o.status.is_fill()).count();
    let closed: Vec<_> = report
        .positions
        .iter()
        .filter(|p| p.status != super::types::PositionStatus::Open)
        .collect();
    let mut wins = 0usize;
    let mut gt2 = 0;
    let mut gt5 = 0;
    let mut gt10 = 0;
    let mut best = None;
    let mut worst = None;
    let mut sum_bps: i128 = 0;
    let mut gross_win = alloy_primitives::U256::ZERO;
    let mut gross_loss = alloy_primitives::U256::ZERO;
    for p in &closed {
        let r = return_bps(&p.quote_cost, &p.realized_quote).unwrap_or(0);
        sum_bps += i128::from(r);
        if r > 0 {
            wins += 1;
        }
        if r >= 10_000 {
            gt2 += 1;
        }
        if r >= 40_000 {
            gt5 += 1;
        }
        if r >= 90_000 {
            gt10 += 1;
        }
        best = Some(best.map_or(r, |b: i64| b.max(r)));
        worst = Some(worst.map_or(r, |b: i64| b.min(r)));
        let rec = parse_u256(&p.realized_quote);
        let cost = parse_u256(&p.quote_cost);
        if rec >= cost {
            gross_win += rec - cost;
        } else {
            gross_loss += cost - rec;
        }
    }
    let n = closed.len();
    let pf = if gross_loss.is_zero() {
        None
    } else {
        Some(
            u32::try_from(
                gross_win.saturating_mul(alloy_primitives::U256::from(1_000u64)) / gross_loss,
            )
            .unwrap_or(u32::MAX),
        )
    };
    PolicyPerformance {
        policy_id: report.run.strategy_policy_id.clone(),
        n_orders: entries.len(),
        filled_entries: filled,
        fill_rate_bps: if entries.is_empty() {
            0
        } else {
            u32::try_from(filled.saturating_mul(10_000) / entries.len()).unwrap_or(0)
        },
        trades_closed: n,
        forced_end: report
            .positions
            .iter()
            .filter(|p| p.status == super::types::PositionStatus::ForcedEndOfData)
            .count(),
        net_pnl_quote: crate::state::amt::u256_dec(if gross_win >= gross_loss {
            gross_win - gross_loss
        } else {
            alloy_primitives::U256::ZERO
        }),
        win_rate_bps: u32::try_from(wins.saturating_mul(10_000).checked_div(n).unwrap_or(0))
            .unwrap_or(0),
        expectancy_bps: if n == 0 {
            None
        } else {
            Some((sum_bps / n as i128) as i64)
        },
        profit_factor_milli: pf,
        largest_winner_bps: best,
        largest_loser_bps: worst,
        trades_gt_2x: gt2,
        trades_gt_5x: gt5,
        trades_gt_10x: gt10,
        sample_insufficient: n < 30,
        research_valid: report.run.research_valid,
    }
}
