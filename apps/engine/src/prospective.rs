//! Prospective paper / shadow. No broadcast. No keys. No backfilled live fills.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::domain::{Chain, Launchpad, QualityStatus};
use crate::sim::exec::{simulate_side, SnapshotBook};
use crate::sim::harness::SimulatedOrder;
use crate::sim::models::SimConfig;
use crate::sim::position::SimulatedPosition;
use crate::sim::types::{ExecutionQuality, ExecutionStatus, OrderSide, PositionStatus};
use crate::state::TokenStateSnapshot;

pub const PROSPECTIVE_MODE: &str = "PROSPECTIVE_PAPER";

pub fn in_pons_snipe_window(cfg: &SimConfig, launchpad: Launchpad, age_ms: i64) -> bool {
    launchpad == Launchpad::PonsV2 && age_ms < cfg.fees.pons_snipe_window_ms
}

pub fn clanker_paper_research_valid() -> bool {
    false
}

pub fn shadow_clanker_order(
    chain: Chain,
    token: &str,
    now: DateTime<Utc>,
    amount: &str,
) -> SimulatedOrder {
    let mut result = crate::sim::exec::ExecutionResult::empty(
        OrderSide::Buy,
        now,
        now,
        amount.to_string(),
        "0".into(),
        ExecutionStatus::UnavailableMarketState,
        ExecutionQuality::PartialState,
        false,
        "IMPACT_MODEL_PARTIAL_UNISWAP_V4",
        0,
        0,
    );
    result.research_valid = false;
    SimulatedOrder {
        id: 0,
        simulation_run_id: None,
        policy_id: "SHADOW_ORDER".into(),
        chain,
        token: token.into(),
        side: OrderSide::Buy,
        decision_time: now,
        requested_amount: amount.into(),
        status: ExecutionStatus::UnavailableMarketState,
        feature_vector_id: None,
        security_assessment_id: None,
        candidate_transition_id: None,
        result,
    }
}

pub fn paper_entry(
    snaps: &[TokenStateSnapshot],
    chain: Chain,
    token: &str,
    launchpad: Launchpad,
    now: DateTime<Utc>,
    cfg: &SimConfig,
    quality: QualityStatus,
) -> crate::sim::exec::ExecutionResult {
    if launchpad == Launchpad::ClankerV4 {
        let mut r = shadow_clanker_order(chain, token, now, &cfg.quote_notional).result;
        r.research_valid = false;
        return r;
    }
    if in_pons_snipe_window(cfg, launchpad, snaps.last().map(|s| s.age_ms).unwrap_or(0)) {
        return crate::sim::exec::ExecutionResult::empty(
            OrderSide::Buy,
            now,
            now,
            cfg.quote_notional.clone(),
            "0".into(),
            ExecutionStatus::RejectedQuality,
            ExecutionQuality::Modelled,
            false,
            "PONS_SNIPE_WINDOW",
            0,
            cfg.slippage.adverse_bps,
        );
    }
    paper_entry_at(snaps, chain, token, launchpad, now, now, cfg, quality)
}

#[allow(clippy::too_many_arguments)]
pub fn paper_entry_at(
    snaps: &[TokenStateSnapshot],
    chain: Chain,
    token: &str,
    launchpad: Launchpad,
    decision: DateTime<Utc>,
    as_of: DateTime<Utc>,
    cfg: &SimConfig,
    quality: QualityStatus,
) -> crate::sim::exec::ExecutionResult {
    if launchpad == Launchpad::ClankerV4 {
        let mut r = shadow_clanker_order(chain, token, decision, &cfg.quote_notional).result;
        r.research_valid = false;
        return r;
    }
    if in_pons_snipe_window(cfg, launchpad, snaps.last().map(|s| s.age_ms).unwrap_or(0)) {
        return crate::sim::exec::ExecutionResult::empty(
            OrderSide::Buy,
            decision,
            as_of,
            cfg.quote_notional.clone(),
            "0".into(),
            ExecutionStatus::RejectedQuality,
            ExecutionQuality::Modelled,
            false,
            "PONS_SNIPE_WINDOW",
            0,
            cfg.slippage.adverse_bps,
        );
    }
    let book = SnapshotBook {
        snapshots: snaps,
        as_of,
    };
    simulate_side(
        &book,
        chain,
        token,
        launchpad,
        OrderSide::Buy,
        decision,
        &cfg.quote_notional,
        true,
        cfg,
        true,
        false,
        quality,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn paper_exit(
    snaps: &[TokenStateSnapshot],
    chain: Chain,
    token: &str,
    launchpad: Launchpad,
    now: DateTime<Utc>,
    token_amount: &str,
    cfg: &SimConfig,
    quality: QualityStatus,
    emergency: bool,
) -> crate::sim::exec::ExecutionResult {
    if launchpad == Launchpad::ClankerV4 {
        let mut r = shadow_clanker_order(chain, token, now, token_amount).result;
        r.side = OrderSide::Sell;
        r.research_valid = false;
        return r;
    }
    let book = SnapshotBook {
        snapshots: snaps,
        as_of: now,
    };
    simulate_side(
        &book,
        chain,
        token,
        launchpad,
        OrderSide::Sell,
        now,
        token_amount,
        false,
        cfg,
        false,
        emergency,
        quality,
    )
}

/// Restart: keep open positions; do not open a second entry for the same token.
pub fn tokens_with_open_positions(positions: &[SimulatedPosition]) -> HashSet<(Chain, String)> {
    positions
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                PositionStatus::Open | PositionStatus::SessionEndedOpen
            )
        })
        .map(|p| (p.chain, p.token.clone()))
        .collect()
}

pub fn mark_session_ended(positions: &mut [SimulatedPosition], at: DateTime<Utc>) {
    for p in positions {
        p.end_session_open(at);
    }
}
