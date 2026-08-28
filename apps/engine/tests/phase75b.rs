use chrono::{TimeZone, Utc};
use memecoin_engine::domain::{Chain, Launchpad};
use memecoin_engine::lab::pons_exp::{
    arm_belongs_to, arm_id_for, experiment_arm_like, EXP002_EXITQUAL_ID, EXP002_ID,
};
use memecoin_engine::lab::reconcile::{reconcile_position, FillLeg};
use memecoin_engine::sim::exec::ExecutionResult;
use memecoin_engine::sim::position::SimulatedPosition;
use memecoin_engine::sim::types::{
    ExecutionQuality, ExecutionStatus, ExitReason, OrderSide, PositionEventKind, PositionStatus,
};

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn fill(side: OrderSide, token: &str, quote: &str, status: ExecutionStatus) -> ExecutionResult {
    let mut r = ExecutionResult::empty(
        side,
        ts(1_000),
        ts(1_100),
        quote.into(),
        token.into(),
        status,
        ExecutionQuality::Modelled,
        true,
        "",
        1,
        0,
    );
    if status.is_fill() {
        r.filled_token = token.into();
        r.filled_quote = quote.into();
        r.actual_simulated_fill_time = Some(ts(1_100));
    }
    r
}

fn open_pos(token_amt: &str, quote: &str) -> SimulatedPosition {
    SimulatedPosition::open(
        1,
        Chain::Robinhood,
        "0xabc".into(),
        Launchpad::PonsV2,
        arm_id_for(
            EXP002_EXITQUAL_ID,
            "P0_FIRST_ELIGIBLE_CONTROL",
            "X2_TIME_5M",
        ),
        &fill(OrderSide::Buy, token_amt, quote, ExecutionStatus::Filled),
        None,
        None,
    )
}

#[test]
fn entry_fill_creates_one_open_position() {
    let p = open_pos("1000", "1000000000");
    assert_eq!(p.status, PositionStatus::Open);
    assert_eq!(p.initial_token_amount, "1000");
    assert_eq!(p.remaining_token_amount, "1000");
    assert_eq!(p.events.len(), 1);
    assert_eq!(p.events[0].kind, PositionEventKind::PositionOpened);
}

#[test]
fn successful_sell_creates_exit_fill_and_closes_at_zero() {
    let mut p = open_pos("1000", "1000000000");
    let sell = fill(
        OrderSide::Sell,
        "1000",
        "900000000",
        ExecutionStatus::Filled,
    );
    p.apply_exit(&sell, ExitReason::TimeStop, true);
    assert_eq!(p.status, PositionStatus::Closed);
    assert_eq!(p.remaining_token_amount, "0");
    assert_eq!(
        p.events.last().unwrap().kind,
        PositionEventKind::PositionClosed
    );
}

#[test]
fn failed_sell_leaves_position_open() {
    let mut p = open_pos("1000", "1000000000");
    let sell = fill(
        OrderSide::Sell,
        "0",
        "0",
        ExecutionStatus::UnavailableMarketState,
    );
    p.apply_exit(&sell, ExitReason::TimeStop, true);
    assert_eq!(p.status, PositionStatus::Open);
    assert_eq!(p.remaining_token_amount, "1000");
    assert_eq!(
        p.events.last().unwrap().kind,
        PositionEventKind::ExitAttemptFailed
    );
}

#[test]
fn partial_sell_reduces_inventory_exactly() {
    let mut p = open_pos("1000", "1000000000");
    let sell = fill(
        OrderSide::Sell,
        "200",
        "250000000",
        ExecutionStatus::PartialFill,
    );
    p.apply_exit(&sell, ExitReason::PartialScale, false);
    assert_eq!(p.status, PositionStatus::Open);
    assert_eq!(p.remaining_token_amount, "800");
    assert_eq!(
        p.events.last().unwrap().kind,
        PositionEventKind::PartialExit
    );
}

#[test]
fn multiple_partial_exits_reconcile() {
    let mut p = open_pos("1000", "1000000000");
    p.apply_exit(
        &fill(
            OrderSide::Sell,
            "200",
            "250000000",
            ExecutionStatus::PartialFill,
        ),
        ExitReason::PartialScale,
        false,
    );
    p.apply_exit(
        &fill(
            OrderSide::Sell,
            "200",
            "300000000",
            ExecutionStatus::PartialFill,
        ),
        ExitReason::PartialScale,
        false,
    );
    p.apply_exit(
        &fill(OrderSide::Sell, "600", "400000000", ExecutionStatus::Filled),
        ExitReason::Trail,
        true,
    );
    assert_eq!(p.status, PositionStatus::Closed);
    assert_eq!(p.remaining_token_amount, "0");
    let rec = reconcile_position(
        &p,
        &[FillLeg {
            token: "1000".into(),
            quote: "1000000000".into(),
        }],
        &[
            FillLeg {
                token: "200".into(),
                quote: "250000000".into(),
            },
            FillLeg {
                token: "200".into(),
                quote: "300000000".into(),
            },
            FillLeg {
                token: "600".into(),
                quote: "400000000".into(),
            },
        ],
    );
    assert!(rec.ok(), "{rec:?}");
    assert_eq!(rec.remaining, "0");
    assert_eq!(rec.sold_token, "1000");
}

#[test]
fn full_flag_does_not_close_with_remainder() {
    let mut p = open_pos("1000", "1000000000");
    p.apply_exit(
        &fill(OrderSide::Sell, "400", "100", ExecutionStatus::PartialFill),
        ExitReason::TimeStop,
        true,
    );
    assert_eq!(p.status, PositionStatus::Open);
    assert_eq!(p.remaining_token_amount, "600");
}

#[test]
fn oversell_is_capped_no_negative_inventory() {
    let mut p = open_pos("1000", "1000000000");
    p.apply_exit(
        &fill(OrderSide::Sell, "5000", "1", ExecutionStatus::Filled),
        ExitReason::TimeStop,
        true,
    );
    assert_eq!(p.remaining_token_amount, "0");
    assert_eq!(p.status, PositionStatus::Closed);
}

#[test]
fn pnl_reconstructed_from_fills() {
    let mut p = open_pos("1000", "1000");
    p.apply_exit(
        &fill(OrderSide::Sell, "1000", "800", ExecutionStatus::Filled),
        ExitReason::TimeStop,
        true,
    );
    let rec = reconcile_position(
        &p,
        &[FillLeg {
            token: "1000".into(),
            quote: "1000".into(),
        }],
        &[FillLeg {
            token: "1000".into(),
            quote: "800".into(),
        }],
    );
    assert_eq!(rec.realized_pnl, "-200");
    assert!(rec.pnl_ok);
}

#[test]
fn closed_without_exit_fill_rejected() {
    let mut p = open_pos("1000", "1000");
    p.status = PositionStatus::Closed;
    p.remaining_token_amount = "0".into();
    let rec = reconcile_position(
        &p,
        &[FillLeg {
            token: "1000".into(),
            quote: "1000".into(),
        }],
        &[],
    );
    assert!(rec.closed_without_exit_fill);
    assert!(!rec.ok());
}

#[test]
fn oversold_rejected_by_reconcile() {
    let p = open_pos("1000", "1000");
    let rec = reconcile_position(
        &p,
        &[FillLeg {
            token: "1000".into(),
            quote: "1000".into(),
        }],
        &[FillLeg {
            token: "1500".into(),
            quote: "1".into(),
        }],
    );
    assert!(rec.oversold);
    assert!(!rec.ok());
}

#[test]
fn restart_then_exit_closes_at_zero() {
    let mut p = open_pos("1000", "1000000000");
    p.end_session_open(ts(5_000));
    assert_eq!(p.status, PositionStatus::SessionEndedOpen);
    p.status = PositionStatus::Open;
    p.apply_exit(
        &fill(OrderSide::Sell, "1000", "1", ExecutionStatus::Filled),
        ExitReason::TimeStop,
        true,
    );
    assert_eq!(p.status, PositionStatus::Closed);
    assert_eq!(p.remaining_token_amount, "0");
}

#[test]
fn exitqual_namespace_does_not_match_exp002() {
    let like = experiment_arm_like(EXP002_ID);
    let prefix = like.trim_end_matches('%');
    let q = arm_id_for(
        EXP002_EXITQUAL_ID,
        "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
        "X6_PARTIAL_RUNNER",
    );
    let e = arm_id_for(
        EXP002_ID,
        "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
        "X6_PARTIAL_RUNNER",
    );
    assert!(e.starts_with(prefix));
    assert!(!q.starts_with(prefix));
    assert!(arm_belongs_to(&q, EXP002_EXITQUAL_ID));
    assert!(!arm_belongs_to(&q, EXP002_ID));
}

#[test]
fn time_exit_uses_wall_hold_not_feature_age() {
    use memecoin_engine::sim::policy::exit_policy;
    use memecoin_engine::sim::position::PositionManager;
    let mut p = open_pos("1000", "1000000000");
    p.opened_at = ts(0);
    let policy = exit_policy("X2_TIME_5M");
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let mgr = PositionManager {
        policy: policy.as_ref(),
        fees: &fees,
    };
    let mut snap = memecoin_engine::state::TokenStateSnapshot {
        id: None,
        chain: Chain::Robinhood,
        token_address: "0xabc".into(),
        launchpad: Launchpad::PonsV2,
        snapshot_time: ts(60_000),
        age_ms: 60_000,
        snapshot_kind: memecoin_engine::state::snapshot::SnapshotKind::Periodic,
        lifecycle_trigger: None,
        lifecycle_state: memecoin_engine::state::lifecycle::TokenLifecycleState::CurveActive,
        quote_asset: None,
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
        last_trade_token_decimals: None,
        last_trade_quote_decimals: None,
        curve_progress_bps: None,
        graduation_progress_bps: None,
        market_state_type: "UNKNOWN".into(),
        market_state: memecoin_engine::state::market::MarketState::Unknown,
        rolling_5s: Default::default(),
        rolling_15s: Default::default(),
        rolling_30s: Default::default(),
        rolling_60s: Default::default(),
        rolling_120s: Default::default(),
        rolling_300s: Default::default(),
        rolling_900s: Default::default(),
        as_of_event_id: None,
        as_of_block: None,
        as_of_slot: None,
        as_of_event_order: "t".into(),
        data_quality: memecoin_engine::domain::QualityStatus::LiveComplete,
        source_session_id: None,
        canonical_status: memecoin_engine::domain::CanonicalStatus::Canonical,
        finality: memecoin_engine::domain::Finality::Confirmed,
        version: 1,
        superseded: false,
        fingerprint: String::new(),
        created_at: ts(60_000),
        wallet: Default::default(),
    };
    assert!(mgr.evaluate(&p, &snap, None, None).is_none());
    snap.snapshot_time = ts(300_000);
    let hit = mgr.evaluate(&p, &snap, None, None).expect("time stop");
    assert_eq!(hit.0, ExitReason::TimeStop);
    assert!(hit.2);
}

#[test]
fn exit_audit_labels_do_not_change_policy() {
    assert_eq!(
        ExitReason::PartialScale.audit_label("X6_PARTIAL_RUNNER"),
        "PARTIAL_TAKE_PROFIT"
    );
    assert_eq!(
        ExitReason::MomentumDecay.audit_label("X9_DYNAMIC_RUNNER"),
        "FLOW_DECAY"
    );
    assert_eq!(ExitReason::Trail.audit_label("X9_DYNAMIC_RUNNER"), "TRAIL");
    assert_eq!(
        ExitReason::TimeStop.audit_label("X9_DYNAMIC_RUNNER"),
        "TIME_CAP"
    );
    assert_eq!(ExitReason::TimeStop.audit_label("X2_TIME_5M"), "TIME_STOP");
    assert_eq!(
        ExitReason::CreatorSell.audit_label("X9_DYNAMIC_RUNNER"),
        "CREATOR_EXIT"
    );
}
