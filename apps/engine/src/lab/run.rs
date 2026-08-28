//! Strategy-aware historical run. Uses Phase 6 fill math unchanged.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::candidate::CandidateState;
use crate::domain::{Chain, QualityStatus};
use crate::features::FeatureVector;
use crate::metrics::DiscoveryMetrics;
use crate::security::assessment::SecurityVerdict;
use crate::sim::exec::{simulate_side, SnapshotBook};
use crate::sim::harness::{run_historical, SimulatedOrder, SimulationReport};
use crate::sim::models::SimConfig;
use crate::sim::policy::exit_policy;
use crate::sim::position::{FlowSignal, PositionManager};
use crate::sim::types::{OrderSide, PositionStatus, SimulationMode, SimulationRun};
use crate::sim::ExitPolicy;
use crate::state::TokenStateSnapshot;
use crate::strategy::{EntryStrategy, StrategyContext};

pub fn feature_at<'a>(
    feats: &'a [FeatureVector],
    chain: Chain,
    token: &str,
    t: DateTime<Utc>,
) -> Option<&'a FeatureVector> {
    feats
        .iter()
        .filter(|f| f.chain == chain && f.token_address == token && f.as_of_time <= t)
        .max_by_key(|f| f.as_of_time)
}

pub fn run_with_strategy(
    snaps: &[TokenStateSnapshot],
    features: &[FeatureVector],
    eligible_at: HashMap<(Chain, String), DateTime<Utc>>,
    security_timeline: &[(DateTime<Utc>, Chain, String, SecurityVerdict)],
    candidate_timeline: &[(DateTime<Utc>, Chain, String, CandidateState)],
    entry: &dyn EntryStrategy,
    exit: &dyn ExitPolicy,
    cfg: &SimConfig,
    quality: QualityStatus,
    seed: u64,
    experiment_id: Option<String>,
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
        format!("{}_{}", entry.id(), exit.id()),
        quality,
        seed,
        serde_json::to_value(cfg).unwrap_or(serde_json::json!({})),
    );
    run.experiment_id = experiment_id;
    run.research_valid = quality.is_research_complete();
    DiscoveryMetrics::experiment_run();

    let mut next_pos = 1i64;
    let mut next_ord = 1i64;
    let mut orders = Vec::new();
    let mut positions = Vec::new();
    let mut attempts = Vec::new();
    let mut entered: HashMap<(Chain, String), bool> = HashMap::new();
    let mut prev_buyers: HashMap<String, u64> = HashMap::new();
    let mut prev_sellers: HashMap<String, u64> = HashMap::new();

    for s in snaps {
        let sec = security_timeline
            .iter()
            .filter(|r| r.1 == s.chain && r.2 == s.token_address && r.0 <= s.snapshot_time)
            .max_by_key(|r| r.0)
            .map(|r| r.3);
        let cand = candidate_timeline
            .iter()
            .filter(|r| r.1 == s.chain && r.2 == s.token_address && r.0 <= s.snapshot_time)
            .max_by_key(|r| r.0)
            .map(|r| r.3)
            .unwrap_or(CandidateState::Discovered);
        let key = (s.chain, s.token_address.clone());
        let first = eligible_at.get(&key).copied();
        let feat = feature_at(features, s.chain, &s.token_address, s.snapshot_time);
        let ctx = StrategyContext {
            features: feat,
            candidate: cand,
            security: sec,
            first_eligible_at: first,
            now: s.snapshot_time,
            token: &s.token_address,
            seed,
        };
        if !entered.get(&key).copied().unwrap_or(false) {
            let d = entry.decide(&ctx);
            if d.enter {
                DiscoveryMetrics::strategy_signal(entry.id());
                let fill = simulate_side(
                    &book,
                    s.chain,
                    &s.token_address,
                    s.launchpad,
                    OrderSide::Buy,
                    s.snapshot_time,
                    &cfg.quote_notional,
                    true,
                    cfg,
                    true,
                    false,
                    quality,
                );
                attempts.push(fill.clone());
                orders.push(SimulatedOrder {
                    id: next_ord,
                    simulation_run_id: run.id,
                    policy_id: format!("{}_{}", entry.id(), exit.id()),
                    chain: s.chain,
                    token: s.token_address.clone(),
                    side: OrderSide::Buy,
                    decision_time: s.snapshot_time,
                    requested_amount: cfg.quote_notional.clone(),
                    status: fill.status,
                    feature_vector_id: feat.and_then(|f| f.id),
                    security_assessment_id: None,
                    candidate_transition_id: None,
                    result: fill.clone(),
                });
                next_ord += 1;
                if fill.status.is_fill() {
                    DiscoveryMetrics::strategy_entry(entry.id());
                    entered.insert(key.clone(), true);
                    let mut pos = crate::sim::SimulatedPosition::open(
                        next_pos,
                        s.chain,
                        s.token_address.clone(),
                        s.launchpad,
                        format!("{}_{}", entry.id(), exit.id()),
                        &fill,
                        feat.and_then(|f| f.id),
                        None,
                    );
                    pos.simulation_run_id = run.id;
                    pos.creator_sell_count_at_entry = s.creator_sell_count;
                    next_pos += 1;
                    positions.push(pos);
                }
            }
        }

        let pb = prev_buyers.get(&s.token_address).copied();
        let ps = prev_sellers.get(&s.token_address).copied();
        let flow = FlowSignal {
            unique_buyer_accel_15s: pb.map(|p| s.rolling_15s.unique_buyers as i64 - p as i64),
            unique_seller_accel_15s: ps.map(|p| s.rolling_15s.unique_sellers as i64 - p as i64),
            net_flow_negative: s.rolling_15s.net_quote_flow.starts_with('-'),
            creator_sell_count: s.creator_sell_count,
        };
        prev_buyers.insert(s.token_address.clone(), s.rolling_15s.unique_buyers);
        prev_sellers.insert(s.token_address.clone(), s.rolling_15s.unique_sellers);

        let mgr = PositionManager {
            policy: exit,
            fees: &cfg.fees,
        };
        let mut exits = Vec::new();
        for (i, p) in positions.iter_mut().enumerate() {
            if p.chain != s.chain || p.token != s.token_address || p.status != PositionStatus::Open
            {
                continue;
            }
            p.mark(s, &cfg.fees);
            if let Some(x) = mgr.evaluate(p, s, sec, Some(&flow)) {
                exits.push((i, x.0, x.1, x.2));
            }
        }
        for (i, reason, amt, full) in exits {
            let p = &positions[i];
            let fill = simulate_side(
                &book,
                p.chain,
                &p.token,
                p.launchpad,
                OrderSide::Sell,
                s.snapshot_time,
                &amt,
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
                requested_amount: amt,
                status: fill.status,
                feature_vector_id: None,
                security_assessment_id: None,
                candidate_transition_id: None,
                result: fill.clone(),
            });
            next_ord += 1;
            positions[i].apply_exit(&fill, reason, full);
        }
    }
    for p in &mut positions {
        if p.status == PositionStatus::Open {
            let last = snaps
                .iter()
                .rev()
                .find(|s| s.chain == p.chain && s.token_address == p.token);
            let realizable = last
                .and_then(|s| crate::sim::mark_exit_quote(s, &p.remaining_token_amount, &cfg.fees))
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

pub fn run_baseline(
    snaps: &[TokenStateSnapshot],
    eligible_at: HashMap<(Chain, String), DateTime<Utc>>,
    security: &[(DateTime<Utc>, Chain, String, SecurityVerdict)],
    candidate: &[(DateTime<Utc>, Chain, String, CandidateState)],
    entry: crate::sim::policy::EntryPolicyId,
    exit_id: &str,
    cfg: &SimConfig,
    quality: QualityStatus,
    seed: u64,
) -> SimulationReport {
    let x = exit_policy(exit_id);
    run_historical(
        snaps,
        eligible_at,
        security,
        candidate,
        entry,
        x.as_ref(),
        cfg,
        quality,
        seed,
    )
}
