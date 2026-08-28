//! Historical simulation harness. Walks snapshots in logical time; no lookahead on decisions.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::candidate::CandidateState;
use crate::domain::{Chain, QualityStatus};
use crate::security::assessment::SecurityVerdict;
use crate::state::TokenStateSnapshot;

use super::exec::{simulate_side, EntryRequest, ExecutionResult, ExitRequest, SnapshotBook};
use super::models::SimConfig;
use super::policy::{may_enter, EntryPolicyId};
use super::position::{ExitPolicy, PositionManager, SimulatedPosition};

use super::types::{
    ExecutionStatus, ExitReason, OrderSide, PositionStatus, SimulationMode, SimulationRun,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimulatedOrder {
    pub id: i64,
    pub simulation_run_id: Option<i64>,
    pub policy_id: String,
    pub chain: Chain,
    pub token: String,
    pub side: OrderSide,
    pub decision_time: DateTime<Utc>,
    pub requested_amount: String,
    pub status: ExecutionStatus,
    pub feature_vector_id: Option<i64>,
    pub security_assessment_id: Option<i64>,
    pub candidate_transition_id: Option<i64>,
    pub result: ExecutionResult,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimulationReport {
    pub run: SimulationRun,
    pub orders: Vec<SimulatedOrder>,
    pub positions: Vec<SimulatedPosition>,
    pub attempts: Vec<ExecutionResult>,
}

pub fn run_historical(
    snaps: &[TokenStateSnapshot],
    eligible_at: HashMap<(Chain, String), DateTime<Utc>>,
    security_timeline: &[(DateTime<Utc>, Chain, String, SecurityVerdict)],
    candidate_timeline: &[(DateTime<Utc>, Chain, String, CandidateState)],
    entry: EntryPolicyId,
    exit: &dyn ExitPolicy,
    cfg: &SimConfig,
    quality: QualityStatus,
    seed: u64,
) -> SimulationReport {
    let end = snaps
        .last()
        .map(|s| s.snapshot_time)
        .unwrap_or_else(Utc::now);
    let book = SnapshotBook {
        snapshots: snaps,
        as_of: end,
    };
    let mut run = SimulationRun::new(
        SimulationMode::Historical,
        format!("{}_{}", entry.as_str(), exit.id()),
        quality,
        seed,
        serde_json::to_value(cfg).unwrap_or(serde_json::json!({})),
    );
    if !quality.is_research_complete() {
        run.research_valid = false;
    }

    let mut next_pos = 1i64;
    let mut next_ord = 1i64;
    let mut orders = Vec::new();
    let mut positions: Vec<SimulatedPosition> = Vec::new();
    let mut attempts = Vec::new();
    let mut entered: HashMap<(Chain, String), bool> = HashMap::new();

    for s in snaps {
        let sec = latest_sec(
            security_timeline,
            s.chain,
            &s.token_address,
            s.snapshot_time,
        );
        let cand = latest_cand(
            candidate_timeline,
            s.chain,
            &s.token_address,
            s.snapshot_time,
        )
        .unwrap_or(CandidateState::Discovered);
        let key = (s.chain, s.token_address.clone());
        let first = eligible_at.get(&key).copied();

        if !entered.get(&key).copied().unwrap_or(false) {
            if let Ok(true) = may_enter(
                entry,
                cand,
                sec,
                first,
                s.snapshot_time,
                &s.token_address,
                seed,
            ) {
                let req = EntryRequest {
                    chain: s.chain,
                    token: s.token_address.clone(),
                    launchpad: s.launchpad,
                    decision_time: s.snapshot_time,
                    feature_vector_id: None,
                    candidate_transition_id: None,
                    security_assessment_id: None,
                    side: OrderSide::Buy,
                    quote_notional: cfg.quote_notional.clone(),
                    max_slippage_bps: cfg.max_slippage_bps,
                    strategy_policy_id: entry.as_str().into(),
                    simulation_run_id: run.id,
                };
                let fill = simulate_side(
                    &book,
                    req.chain,
                    &req.token,
                    req.launchpad,
                    OrderSide::Buy,
                    req.decision_time,
                    &req.quote_notional,
                    true,
                    cfg,
                    true,
                    false,
                    quality,
                );
                attempts.push(fill.clone());
                let ord = SimulatedOrder {
                    id: next_ord,
                    simulation_run_id: run.id,
                    policy_id: format!("{}_{}", entry.as_str(), exit.id()),
                    chain: s.chain,
                    token: s.token_address.clone(),
                    side: OrderSide::Buy,
                    decision_time: s.snapshot_time,
                    requested_amount: cfg.quote_notional.clone(),
                    status: fill.status,
                    feature_vector_id: None,
                    security_assessment_id: None,
                    candidate_transition_id: None,
                    result: fill.clone(),
                };
                next_ord += 1;
                if fill.status.is_fill() {
                    entered.insert(key.clone(), true);
                    let mut pos = SimulatedPosition::open(
                        next_pos,
                        s.chain,
                        s.token_address.clone(),
                        s.launchpad,
                        format!("{}_{}", entry.as_str(), exit.id()),
                        &fill,
                        None,
                        None,
                    );
                    pos.simulation_run_id = run.id;
                    pos.creator_sell_count_at_entry = s.creator_sell_count;
                    next_pos += 1;
                    crate::metrics::DiscoveryMetrics::sim_position_open();
                    positions.push(pos);
                }
                orders.push(ord);
            }
        }

        let mgr = PositionManager {
            policy: exit,
            fees: &cfg.fees,
        };
        let mut exits: Vec<(usize, ExitReason, String, bool)> = Vec::new();
        for (i, p) in positions.iter_mut().enumerate() {
            if p.chain != s.chain || p.token != s.token_address {
                continue;
            }
            if p.status != PositionStatus::Open {
                continue;
            }
            p.mark(s, &cfg.fees);
            let flow = super::position::FlowSignal {
                unique_buyer_accel_15s: None,
                unique_seller_accel_15s: None,
                net_flow_negative: s.rolling_15s.net_quote_flow.starts_with('-'),
                creator_sell_count: s.creator_sell_count,
            };
            if let Some((reason, amt, full)) = mgr.evaluate(p, s, sec, Some(&flow)) {
                exits.push((i, reason, amt, full));
            }
        }
        for (i, reason, amt, full) in exits {
            let p = &positions[i];
            let xr = ExitRequest {
                position_id: p.id,
                chain: p.chain,
                token: p.token.clone(),
                launchpad: p.launchpad,
                decision_time: s.snapshot_time,
                token_amount_requested: amt,
                reason,
                max_slippage_bps: if reason.is_emergency() {
                    50_000
                } else {
                    cfg.max_slippage_bps
                },
                simulation_run_id: run.id,
            };
            let fill = simulate_side(
                &book,
                xr.chain,
                &xr.token,
                xr.launchpad,
                OrderSide::Sell,
                xr.decision_time,
                &xr.token_amount_requested,
                false,
                cfg,
                false,
                reason.is_emergency(),
                quality,
            );
            attempts.push(fill.clone());
            orders.push(SimulatedOrder {
                id: next_ord,
                simulation_run_id: run.id,
                policy_id: p.strategy_policy_id.clone(),
                chain: p.chain,
                token: p.token.clone(),
                side: OrderSide::Sell,
                decision_time: s.snapshot_time,
                requested_amount: xr.token_amount_requested.clone(),
                status: fill.status,
                feature_vector_id: None,
                security_assessment_id: None,
                candidate_transition_id: None,
                result: fill.clone(),
            });
            next_ord += 1;
            positions[i].apply_exit(&fill, reason, full);
            if positions[i].status == PositionStatus::Closed {
                crate::metrics::DiscoveryMetrics::sim_position_closed();
            }
        }
    }

    for p in &mut positions {
        if p.status == PositionStatus::Open {
            let last = snaps
                .iter()
                .rev()
                .find(|s| s.chain == p.chain && s.token_address == p.token);
            let realizable = last
                .and_then(|s| {
                    super::impact::mark_exit_quote(s, &p.remaining_token_amount, &cfg.fees)
                })
                .is_some();
            p.force_end(end, realizable);
        }
    }

    run.ended_at = Some(Utc::now());
    SimulationReport {
        run,
        orders,
        positions,
        attempts,
    }
}

fn latest_sec(
    rows: &[(DateTime<Utc>, Chain, String, SecurityVerdict)],
    chain: Chain,
    token: &str,
    t: DateTime<Utc>,
) -> Option<SecurityVerdict> {
    rows.iter()
        .filter(|r| r.1 == chain && r.2 == token && r.0 <= t)
        .max_by_key(|r| r.0)
        .map(|r| r.3)
}

fn latest_cand(
    rows: &[(DateTime<Utc>, Chain, String, CandidateState)],
    chain: Chain,
    token: &str,
    t: DateTime<Utc>,
) -> Option<CandidateState> {
    rows.iter()
        .filter(|r| r.1 == chain && r.2 == token && r.0 <= t)
        .max_by_key(|r| r.0)
        .map(|r| r.3)
}
