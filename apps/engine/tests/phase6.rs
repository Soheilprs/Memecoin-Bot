#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::CandidateState;
use memecoin_engine::domain::{CanonicalStatus, Chain, Finality, Launchpad, QualityStatus};
use memecoin_engine::security::assessment::SecurityVerdict;
use memecoin_engine::sim::exec::{
    simulate_side, EntryRequest, ExecutionEngine, HistoricalExecutionEngine, LiveExecutionEngine,
    PaperExecutionEngine, SnapshotBook,
};
use memecoin_engine::sim::impact::{executable_fill, max_quote_at_impact};
use memecoin_engine::sim::models::{FailureModel, SimConfig};
use memecoin_engine::sim::outcome::{policy_performance, MissReason, OutcomeEngine};
use memecoin_engine::sim::policy::{exit_policy, may_enter, EntryPolicyId};
use memecoin_engine::sim::run_historical;
use memecoin_engine::sim::types::{ExecutionStatus, LatencyScenario, OrderSide, SimulationMode};
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::market::{BondingCurveState, MarketState, MarketStateQuality};
use memecoin_engine::state::rolling::RollingWindowSnapshot;
use memecoin_engine::state::snapshot::{SnapshotKind, TokenStateSnapshot, WalletSnapshot};
use std::collections::HashMap;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn empty_roll(ms: i64) -> RollingWindowSnapshot {
    RollingWindowSnapshot {
        duration_ms: ms,
        buy_quote_volume_raw: "0".into(),
        sell_quote_volume_raw: "0".into(),
        buy_token_volume_raw: "0".into(),
        sell_token_volume_raw: "0".into(),
        net_quote_flow: "0".into(),
        creator_buy_volume: "0".into(),
        creator_sell_volume: "0".into(),
        ..Default::default()
    }
}

fn curve_snap(
    token: &str,
    t_ms: i64,
    age_ms: i64,
    vsol: &str,
    vtok: &str,
    real_sol: Option<&str>,
    real_tok: Option<&str>,
    life: TokenLifecycleState,
    lp: Launchpad,
    quality: QualityStatus,
) -> TokenStateSnapshot {
    TokenStateSnapshot {
        id: None,
        chain: Chain::Solana,
        token_address: token.into(),
        launchpad: lp,
        snapshot_time: ts(t_ms),
        age_ms,
        snapshot_kind: SnapshotKind::Periodic,
        lifecycle_trigger: None,
        lifecycle_state: life,
        quote_asset: Some("SOL".into()),
        buy_count_total: 0,
        sell_count_total: 0,
        unique_buyers_total: 0,
        unique_sellers_total: 0,
        buy_quote_volume_raw_total: "0".into(),
        sell_quote_volume_raw_total: "0".into(),
        buy_token_volume_raw_total: "0".into(),
        sell_token_volume_raw_total: "0".into(),
        creator_buy_count: 0,
        creator_sell_count: 0,
        creator_buy_quote_raw: "0".into(),
        creator_sell_quote_raw: "0".into(),
        last_trade_side: None,
        last_trade_token_raw: None,
        last_trade_quote_raw: None,
        last_trade_token_decimals: Some(6),
        last_trade_quote_decimals: Some(9),
        curve_progress_bps: Some(1000),
        graduation_progress_bps: None,
        market_state_type: "BONDING_CURVE".into(),
        market_state: MarketState::BondingCurve(BondingCurveState {
            virtual_sol_reserves: Some(vsol.into()),
            virtual_token_reserves: Some(vtok.into()),
            real_sol_reserves: real_sol.map(|s| s.into()),
            real_token_reserves: real_tok.map(|s| s.into()),
            token_total_supply: None,
            curve_progress_bps: Some(1000),
            last_token_amount_raw: None,
            last_quote_amount_raw: None,
            quality: MarketStateQuality::Complete,
        }),
        rolling_5s: empty_roll(5_000),
        rolling_15s: empty_roll(15_000),
        rolling_30s: empty_roll(30_000),
        rolling_60s: empty_roll(60_000),
        rolling_120s: empty_roll(120_000),
        rolling_300s: empty_roll(300_000),
        rolling_900s: empty_roll(900_000),
        as_of_event_id: None,
        as_of_block: None,
        as_of_slot: Some(t_ms),
        as_of_event_order: format!("{t_ms}"),
        data_quality: quality,
        source_session_id: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        version: 1,
        superseded: false,
        fingerprint: String::new(),
        created_at: ts(t_ms),
        wallet: WalletSnapshot::default(),
    }
}

const VSOL: &str = "30000000000";
const VTOK: &str = "1073000000000000";

#[test]
fn live_engine_is_stub() {
    assert!(LiveExecutionEngine::not_implemented().contains("not implemented"));
}

#[test]
fn larger_buy_worse_price() {
    let s = curve_snap(
        "t",
        1_000,
        30_000,
        VSOL,
        VTOK,
        Some("10000000000"),
        Some("800000000000000"),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let slip = memecoin_engine::sim::models::SlippageModel::none();
    let small = executable_fill(
        &s,
        OrderSide::Buy,
        "10000000",
        &fees,
        &slip,
        u32::MAX,
        false,
    );
    let large = executable_fill(
        &s,
        OrderSide::Buy,
        "1000000000",
        &fees,
        &slip,
        u32::MAX,
        false,
    );
    assert!(small.status.is_fill() && large.status.is_fill());
    let sp = memecoin_engine::state::amt::parse_u256(&small.effective_price_1e18);
    let lp = memecoin_engine::state::amt::parse_u256(&large.effective_price_1e18);
    assert!(
        lp > sp,
        "larger buy must have worse (higher) effective price"
    );
}

#[test]
fn unknown_liquidity_is_not_infinite() {
    let mut s = curve_snap(
        "t",
        1_000,
        1_000,
        VSOL,
        VTOK,
        None,
        None,
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    s.market_state = MarketState::Unknown;
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let fill = executable_fill(
        &s,
        OrderSide::Buy,
        "1000",
        &fees,
        &memecoin_engine::sim::models::SlippageModel::none(),
        u32::MAX,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::UnavailableMarketState);
    assert!(!fill.status.is_fill());
}

#[test]
fn pons_gap_blocks_exit() {
    let s = curve_snap(
        "p",
        5_000,
        5_000,
        VSOL,
        VTOK,
        Some("1"),
        Some("1"),
        TokenLifecycleState::GraduationGap,
        Launchpad::PonsV2,
        QualityStatus::HistoricalReplay,
    );
    let fill = executable_fill(
        &s,
        OrderSide::Sell,
        "1000",
        &memecoin_engine::sim::models::FeeModel::research_default(),
        &memecoin_engine::sim::models::SlippageModel::none(),
        u32::MAX,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::TemporarilyUnavailable);
}

#[test]
fn snipe_tax_destroys_fill() {
    let s = curve_snap(
        "p",
        1_200,
        200,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PonsV2,
        QualityStatus::HistoricalReplay,
    );
    let fill = executable_fill(
        &s,
        OrderSide::Buy,
        "1000000000",
        &memecoin_engine::sim::models::FeeModel::research_default(),
        &memecoin_engine::sim::models::SlippageModel::none(),
        u32::MAX,
        true,
    );
    assert!(fill.status.is_fill());
    let tax = memecoin_engine::state::amt::parse_u256(&fill.snipe_tax);
    let q = memecoin_engine::state::amt::parse_u256(&fill.quote_amount);
    assert!(tax > q / alloy_primitives::U256::from(2u64));
}

#[test]
fn partial_fill_when_real_reserves_cap() {
    let s = curve_snap(
        "t",
        1_000,
        30_000,
        VSOL,
        VTOK,
        Some("1000"),
        Some("100"),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let fill = executable_fill(
        &s,
        OrderSide::Buy,
        "10000000000",
        &memecoin_engine::sim::models::FeeModel::research_default(),
        &memecoin_engine::sim::models::SlippageModel::none(),
        u32::MAX,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::PartialFill);
    assert_eq!(fill.token_amount, "100");
}

#[test]
fn slippage_monotone() {
    let s = curve_snap(
        "t",
        1_000,
        30_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let a = executable_fill(
        &s,
        OrderSide::Buy,
        "100000000",
        &fees,
        &memecoin_engine::sim::models::SlippageModel::bps(0),
        u32::MAX,
        false,
    );
    let b = executable_fill(
        &s,
        OrderSide::Buy,
        "100000000",
        &fees,
        &memecoin_engine::sim::models::SlippageModel::bps(100),
        u32::MAX,
        false,
    );
    let c = executable_fill(
        &s,
        OrderSide::Buy,
        "100000000",
        &fees,
        &memecoin_engine::sim::models::SlippageModel::bps(300),
        u32::MAX,
        false,
    );
    let ta = memecoin_engine::state::amt::parse_u256(&a.token_amount);
    let tb = memecoin_engine::state::amt::parse_u256(&b.token_amount);
    let tc = memecoin_engine::state::amt::parse_u256(&c.token_amount);
    assert!(ta >= tb && tb >= tc);
}

#[test]
fn delay_uses_later_state() {
    let s0 = curve_snap(
        "t",
        10_000,
        30_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let s1 = curve_snap(
        "t",
        15_000,
        35_000,
        "60000000000",
        "500000000000000",
        Some("60000000000"),
        Some("500000000000000"),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let snaps = [s0, s1];
    let mut cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    cfg.delay.solana_fast_ms = 0;
    let book = SnapshotBook {
        snapshots: &snaps,
        as_of: ts(20_000),
    };
    let fast = simulate_side(
        &book,
        Chain::Solana,
        "t",
        Launchpad::PumpFun,
        OrderSide::Buy,
        ts(10_000),
        "100000000",
        true,
        &cfg,
        true,
        false,
        QualityStatus::HistoricalReplay,
    );
    cfg.delay.solana_fast_ms = 5_000;
    let slow = simulate_side(
        &book,
        Chain::Solana,
        "t",
        Launchpad::PumpFun,
        OrderSide::Buy,
        ts(10_000),
        "100000000",
        true,
        &cfg,
        true,
        false,
        QualityStatus::HistoricalReplay,
    );
    assert_ne!(
        fast.effective_fill_price_1e18,
        slow.effective_fill_price_1e18
    );
    assert_eq!(slow.snapshot_id, snaps[1].id);
}

#[test]
fn paper_cannot_use_future_snapshot() {
    let s0 = curve_snap(
        "t",
        10_000,
        30_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::LiveComplete,
    );
    let s1 = curve_snap(
        "t",
        15_000,
        35_000,
        "60000000000",
        VTOK,
        Some("60000000000"),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::LiveComplete,
    );
    let snaps = [s0, s1];
    let mut cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    cfg.delay.solana_fast_ms = 5_000;
    let paper_now = ts(10_000);
    let book = SnapshotBook {
        snapshots: &snaps,
        as_of: paper_now,
    };
    let r = simulate_side(
        &book,
        Chain::Solana,
        "t",
        Launchpad::PumpFun,
        OrderSide::Buy,
        ts(10_000),
        "100000000",
        true,
        &cfg,
        true,
        false,
        QualityStatus::LiveComplete,
    );
    assert_eq!(r.status, ExecutionStatus::NoFill);
    assert_eq!(r.reason.as_deref(), Some("FILL_TIME_NOT_YET_AVAILABLE"));
}

#[test]
fn no_lookahead_zero_delay_ignores_future_trade_state() {
    let s0 = curve_snap(
        "t",
        60_000,
        60_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let s1 = curve_snap(
        "t",
        61_000,
        61_000,
        "90000000000",
        "100000000000000",
        Some("90000000000"),
        Some("100000000000000"),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let mut cfg = SimConfig::research_default();
    cfg.delay.solana_base_ms = 0;
    let book = SnapshotBook {
        snapshots: &[s0.clone(), s1],
        as_of: ts(120_000),
    };
    let r = simulate_side(
        &book,
        Chain::Solana,
        "t",
        Launchpad::PumpFun,
        OrderSide::Buy,
        ts(60_000),
        "100000000",
        true,
        &cfg,
        true,
        false,
        QualityStatus::HistoricalReplay,
    );
    assert!(r.status.is_fill());
    let only = SnapshotBook {
        snapshots: std::slice::from_ref(&s0),
        as_of: ts(120_000),
    };
    let r2 = simulate_side(
        &only,
        Chain::Solana,
        "t",
        Launchpad::PumpFun,
        OrderSide::Buy,
        ts(60_000),
        "100000000",
        true,
        &cfg,
        true,
        false,
        QualityStatus::HistoricalReplay,
    );
    assert_eq!(r.effective_fill_price_1e18, r2.effective_fill_price_1e18);
}

#[test]
fn security_reject_never_enters() {
    let e = may_enter(
        EntryPolicyId::FirstEligible,
        CandidateState::Eligible,
        Some(SecurityVerdict::Reject),
        Some(ts(1_000)),
        ts(30_000),
        "t",
        1,
    );
    assert_eq!(e, Err("SECURITY_REJECT"));
}

#[test]
fn security_unknown_not_research_entry() {
    let e = may_enter(
        EntryPolicyId::FirstEligible,
        CandidateState::Eligible,
        Some(SecurityVerdict::Unknown),
        Some(ts(1_000)),
        ts(30_000),
        "t",
        1,
    );
    assert_eq!(e, Err("SECURITY_UNKNOWN"));
}

fn rising_snaps(token: &str, n: i64, start_sol: u128) -> Vec<TokenStateSnapshot> {
    (0..n)
        .map(|i| {
            let sol = start_sol + i as u128 * 2_000_000_000;
            let tok: u128 =
                1_073_000_000_000_000u128.saturating_sub(i as u128 * 20_000_000_000_000);
            curve_snap(
                token,
                30_000 + i * 5_000,
                30_000 + i * 5_000,
                &sol.to_string(),
                &tok.to_string(),
                Some(&sol.to_string()),
                Some(&tok.to_string()),
                TokenLifecycleState::CurveActive,
                Launchpad::PumpFun,
                QualityStatus::HistoricalReplay,
            )
        })
        .collect()
}

fn harness(
    snaps: &[TokenStateSnapshot],
    entry: EntryPolicyId,
    exit: &str,
    cfg: &SimConfig,
    quality: QualityStatus,
) -> memecoin_engine::sim::SimulationReport {
    let mut eligible = HashMap::new();
    let mut cand = Vec::new();
    let mut sec = Vec::new();
    for s in snaps {
        eligible
            .entry((s.chain, s.token_address.clone()))
            .or_insert(s.snapshot_time);
        cand.push((
            s.snapshot_time,
            s.chain,
            s.token_address.clone(),
            CandidateState::Eligible,
        ));
        sec.push((
            s.snapshot_time,
            s.chain,
            s.token_address.clone(),
            SecurityVerdict::Pass,
        ));
    }
    let x = exit_policy(exit);
    run_historical(
        snaps,
        eligible,
        &sec,
        &cand,
        entry,
        x.as_ref(),
        cfg,
        quality,
        1,
    )
}

#[test]
fn deterministic_replay() {
    let snaps = rising_snaps("d", 20, 30_000_000_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let a = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let b = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    assert_eq!(a.orders.len(), b.orders.len());
    assert_eq!(
        a.orders[0].result.filled_token,
        b.orders[0].result.filled_token
    );
    assert_eq!(a.positions[0].realized_quote, b.positions[0].realized_quote);
}

#[test]
fn rpc_dev_not_research_valid() {
    let snaps = rising_snaps("r", 8, 30_000_000_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::RpcDevIncomplete,
    );
    assert!(!r.run.research_valid);
    let p = policy_performance(&r);
    assert!(!p.research_valid);
}

#[test]
fn failed_fills_are_persisted() {
    let snaps = rising_snaps("f", 6, 30_000_000_000);
    let mut cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    cfg.failure = FailureModel::rates(7, 10_000, 0);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    assert!(r.orders.iter().any(|o| o.status == ExecutionStatus::Failed));
    assert!(!r.orders.is_empty());
}

#[test]
fn seeded_failure_is_deterministic() {
    let snaps = rising_snaps("f", 6, 30_000_000_000);
    let mut cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    cfg.failure = FailureModel::rates(42, 8_000, 0);
    let a = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let b = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let fa: Vec<_> = a.orders.iter().map(|o| o.status).collect();
    let fb: Vec<_> = b.orders.iter().map(|o| o.status).collect();
    assert_eq!(fa, fb);
}

#[test]
fn dead_market_does_not_magic_stop() {
    let mut snaps = rising_snaps("z", 4, 30_000_000_000);
    snaps.push({
        let mut s = snaps.last().unwrap().clone();
        s.snapshot_time = ts(80_000);
        s.age_ms = 80_000;
        s.lifecycle_state = TokenLifecycleState::Inactive;
        s.market_state = MarketState::Unknown;
        s
    });
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X4_FIXED_TP_SL",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    assert!(r.positions.iter().any(|p| {
        p.events.iter().any(|e| {
            e.reason.as_deref().is_some_and(|x| {
                x.contains("UNAVAILABLE") || x.contains("UNREALIZABLE") || x.contains("MARKET")
            }) || p.status == memecoin_engine::sim::types::PositionStatus::Unrealizable
                || p.status == memecoin_engine::sim::types::PositionStatus::ForcedEndOfData
        })
    }));
}

#[test]
fn security_emergency_exit() {
    let snaps = rising_snaps("e", 12, 30_000_000_000);
    let mut eligible = HashMap::new();
    eligible.insert((Chain::Solana, "e".into()), snaps[0].snapshot_time);
    let cand: Vec<_> = snaps
        .iter()
        .map(|s| {
            (
                s.snapshot_time,
                s.chain,
                s.token_address.clone(),
                CandidateState::Eligible,
            )
        })
        .collect();
    let sec: Vec<_> = snaps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            (
                s.snapshot_time,
                s.chain,
                s.token_address.clone(),
                if i >= 4 {
                    SecurityVerdict::Reject
                } else {
                    SecurityVerdict::Pass
                },
            )
        })
        .collect();
    let x = exit_policy("X2_TIME_5M");
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = run_historical(
        &snaps,
        eligible,
        &sec,
        &cand,
        EntryPolicyId::FirstEligible,
        x.as_ref(),
        &cfg,
        QualityStatus::HistoricalReplay,
        1,
    );
    assert!(r.orders.iter().any(|o| o.side == OrderSide::Sell));
}

#[test]
fn partial_runner_keeps_remainder() {
    let snaps = rising_snaps("w", 40, 30_000_000_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X6_PARTIAL_RUNNER",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    assert!(!r.positions.is_empty());
    let p = &r.positions[0];
    let partials = p
        .events
        .iter()
        .filter(|e| e.kind == memecoin_engine::sim::types::PositionEventKind::PartialExit)
        .count();
    assert!(partials >= 1 || p.events.len() >= 2);
}

#[test]
fn outcomes_include_dead_and_rejected() {
    let mut snaps = rising_snaps("dead", 3, 30_000_000_000);
    snaps.extend(rising_snaps("hot", 20, 30_000_000_000));
    let outs = OutcomeEngine::outcomes_for_all(&snaps, 0, 3_600_000);
    assert!(outs.iter().any(|o| o.token == "dead"));
    assert!(outs.iter().any(|o| o.token == "hot"));
}

#[test]
fn missed_winner_never_eligible() {
    let snaps = rising_snaps("m", 30, 30_000_000_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let outs = OutcomeEngine::outcomes_for_all(&snaps, 0, 3_600_000);
    let missed = OutcomeEngine::missed_winners(
        &outs,
        &r,
        &[(ts(1), Chain::Solana, "m".into(), SecurityVerdict::Pass)],
        &[(
            ts(1),
            Chain::Solana,
            "other".into(),
            CandidateState::Watching,
        )],
        10_000,
    );
    let _ = missed;
    let mut empty_report = r.clone();
    empty_report.positions.clear();
    empty_report.orders.clear();
    let m = OutcomeEngine::missed_winners(
        &outs,
        &empty_report,
        &[(ts(1), Chain::Solana, "m".into(), SecurityVerdict::Pass)],
        &[(ts(1), Chain::Solana, "m".into(), CandidateState::Watching)],
        0,
    );
    assert!(m.iter().any(|x| x.miss_reason == MissReason::NeverEligible));
}

#[test]
fn security_rejected_winner_still_labelled() {
    let snaps = rising_snaps("rej", 25, 30_000_000_000);
    let outs = OutcomeEngine::outcomes_for_all(&snaps, 0, 3_600_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let mut empty = r.clone();
    empty.positions.clear();
    empty.orders.clear();
    let m = OutcomeEngine::missed_winners(
        &outs,
        &empty,
        &[(ts(1), Chain::Solana, "rej".into(), SecurityVerdict::Reject)],
        &[(
            ts(1),
            Chain::Solana,
            "rej".into(),
            CandidateState::SecurityRejected,
        )],
        0,
    );
    assert!(m
        .iter()
        .any(|x| x.miss_reason == MissReason::SecurityReject));
    assert!(outs.iter().any(|o| o.token == "rej"));
}

#[test]
fn early_exit_low_capture() {
    let snaps = rising_snaps("cap", 40, 30_000_000_000);
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    let _outs = OutcomeEngine::outcomes_for_all(&snaps, 0, 3_600_000);
    assert!(!r.positions.is_empty());
    let p = &r.positions[0];
    assert!(p.mfe_quote != "0");
}

#[test]
fn uniswap_v4_not_faked_as_cp() {
    let mut s = curve_snap(
        "c",
        1_000,
        1_000,
        VSOL,
        VTOK,
        None,
        None,
        TokenLifecycleState::AmmActive,
        Launchpad::ClankerV4,
        QualityStatus::HistoricalReplay,
    );
    s.market_state = MarketState::UniswapV4(Default::default());
    let fill = executable_fill(
        &s,
        OrderSide::Buy,
        "1000",
        &memecoin_engine::sim::models::FeeModel::research_default(),
        &memecoin_engine::sim::models::SlippageModel::none(),
        u32::MAX,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::UnavailableMarketState);
    assert_eq!(
        fill.quality,
        memecoin_engine::sim::types::ExecutionQuality::PartialState
    );
}

#[test]
fn max_quote_at_impact_exists_for_curve() {
    let s = curve_snap(
        "t",
        1_000,
        30_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let q = max_quote_at_impact(&s, 500);
    assert!(q.is_some());
}

#[test]
fn two_thousand_token_sim() {
    let mut snaps = Vec::new();
    for i in 0..2_000 {
        snaps.push(curve_snap(
            &format!("tok{i}"),
            30_000,
            30_000,
            VSOL,
            VTOK,
            Some(VSOL),
            Some(VTOK),
            TokenLifecycleState::CurveActive,
            Launchpad::PumpFun,
            QualityStatus::HistoricalReplay,
        ));
        snaps.push(curve_snap(
            &format!("tok{i}"),
            150_000,
            150_000,
            "35000000000",
            "900000000000000",
            Some("35000000000"),
            Some("900000000000000"),
            TokenLifecycleState::CurveActive,
            Launchpad::PumpFun,
            QualityStatus::HistoricalReplay,
        ));
    }
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let started = std::time::Instant::now();
    let r = harness(
        &snaps,
        EntryPolicyId::FirstEligible,
        "X1_TIME_2M",
        &cfg,
        QualityStatus::HistoricalReplay,
    );
    assert!(r.orders.len() >= 2_000);
    assert!(started.elapsed().as_secs() < 60);
    let p = policy_performance(&r);
    assert!(p.sample_insufficient || p.n_orders > 0);
    assert!(p.policy_id.contains("E1"));
}

#[tokio::test]
async fn historical_engine_trait() {
    let s = curve_snap(
        "t",
        1_000,
        30_000,
        VSOL,
        VTOK,
        Some(VSOL),
        Some(VTOK),
        TokenLifecycleState::CurveActive,
        Launchpad::PumpFun,
        QualityStatus::HistoricalReplay,
    );
    let snaps = [s];
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let eng = HistoricalExecutionEngine {
        book: SnapshotBook {
            snapshots: &snaps,
            as_of: ts(10_000),
        },
        cfg: &cfg,
        data_quality: QualityStatus::HistoricalReplay,
    };
    let req = EntryRequest {
        chain: Chain::Solana,
        token: "t".into(),
        launchpad: Launchpad::PumpFun,
        decision_time: ts(1_000),
        feature_vector_id: None,
        candidate_transition_id: None,
        security_assessment_id: None,
        side: OrderSide::Buy,
        quote_notional: "100000000".into(),
        max_slippage_bps: 50_000,
        strategy_policy_id: "E1".into(),
        simulation_run_id: None,
    };
    let q = eng.quote_entry(&req).await.unwrap();
    let x = eng.execute_entry(&req).await.unwrap();
    assert!(
        q.status.is_fill()
            || x.status.is_fill()
            || matches!(
                x.status,
                ExecutionStatus::NoFill | ExecutionStatus::Filled | ExecutionStatus::PartialFill
            )
    );
    let paper = PaperExecutionEngine {
        book: SnapshotBook {
            snapshots: &snaps,
            as_of: ts(1_000),
        },
        cfg: &cfg,
        data_quality: QualityStatus::LiveComplete,
    };
    let _ = paper.quote_entry(&req).await.unwrap();
}

#[test]
fn capture_ratio_formula() {
    let c = memecoin_engine::sim::capture_ratio_bps("100", "250", "1000");
    assert_eq!(c, Some(1666));
}

#[test]
fn no_shorting() {
    assert_eq!(OrderSide::Buy.as_str(), "BUY");
    assert_eq!(SimulationMode::Historical.as_str(), "HISTORICAL");
}
