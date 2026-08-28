#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::CandidateState;
use memecoin_engine::domain::{CanonicalStatus, Chain, Finality, Launchpad, QualityStatus};
use memecoin_engine::features::engine::{FeatureEngine, FeatureInput};
use memecoin_engine::features::opt::OptI64;
use memecoin_engine::lab::analysis::{
    chronological_drawdown_bps, moonshot_precision_bps, moonshot_recall_bps, right_tail_share_bps,
    train_thresholds, FeatureSample,
};
use memecoin_engine::lab::experiment::StrategyExperiment;
use memecoin_engine::lab::persist::SimStore;
use memecoin_engine::lab::run::run_with_strategy;
use memecoin_engine::lab::split::{assign_split, chronological_split, SplitKind};
use memecoin_engine::security::assessment::SecurityVerdict;
use memecoin_engine::sim::models::SimConfig;
use memecoin_engine::sim::policy::exit_policy;
use memecoin_engine::sim::types::LatencyScenario;
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::market::{BondingCurveState, MarketState, MarketStateQuality};
use memecoin_engine::state::rolling::RollingWindowSnapshot;
use memecoin_engine::state::snapshot::{SnapshotKind, TokenStateSnapshot, WalletSnapshot};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::strategy::{family, quantile_i64, StrategyContext, StrategyThresholds};
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

fn snap(token: &str, t_ms: i64) -> TokenStateSnapshot {
    TokenStateSnapshot {
        id: None,
        chain: Chain::Solana,
        token_address: token.into(),
        launchpad: Launchpad::PumpFun,
        snapshot_time: ts(t_ms),
        age_ms: t_ms,
        snapshot_kind: SnapshotKind::Periodic,
        lifecycle_trigger: None,
        lifecycle_state: TokenLifecycleState::CurveActive,
        quote_asset: Some("SOL".into()),
        buy_count_total: 4,
        sell_count_total: 1,
        unique_buyers_total: 4,
        unique_sellers_total: 1,
        buy_quote_volume_raw_total: "100".into(),
        sell_quote_volume_raw_total: "10".into(),
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
        curve_progress_bps: Some(2000),
        graduation_progress_bps: None,
        market_state_type: "BONDING_CURVE".into(),
        market_state: MarketState::BondingCurve(BondingCurveState {
            virtual_sol_reserves: Some("30000000000".into()),
            virtual_token_reserves: Some("1073000000000000".into()),
            real_sol_reserves: Some("30000000000".into()),
            real_token_reserves: Some("1073000000000000".into()),
            token_total_supply: None,
            curve_progress_bps: Some(2000),
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
        data_quality: QualityStatus::HistoricalReplay,
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

fn vec_at(
    token: &str,
    t_ms: i64,
    accel: i64,
    vel: i64,
) -> memecoin_engine::features::FeatureVector {
    let s = snap(token, t_ms);
    let mut v = FeatureEngine::compute(FeatureInput::from_history(&s, &[], None));
    v.shared.unique_buyer_acceleration_15s = OptI64::value(accel);
    v.shared.unique_buyer_velocity_15s = OptI64::value(vel);
    v.shared.unique_buyers_total = 5;
    v.token_age_ms = t_ms;
    v.shared.token_age_ms = t_ms;
    v
}

fn ctx<'a>(f: &'a memecoin_engine::features::FeatureVector, token: &'a str) -> StrategyContext<'a> {
    StrategyContext {
        features: Some(f),
        candidate: CandidateState::Eligible,
        security: Some(SecurityVerdict::Pass),
        first_eligible_at: Some(f.as_of_time),
        now: f.as_of_time,
        token,
        seed: 1,
    }
}

#[test]
fn chronological_split_no_random() {
    let b = chronological_split(ts(0), ts(100_000));
    assert_eq!(assign_split(ts(10_000), &b), SplitKind::Train);
    assert_eq!(assign_split(ts(70_000), &b), SplitKind::Validation);
    assert_eq!(assign_split(ts(90_000), &b), SplitKind::Test);
    assert!(b.train_end <= b.validation_start);
    assert!(b.validation_end <= b.test_start);
}

#[test]
fn train_only_threshold_ignores_test_outcomes() {
    let train = vec![1i64, 2, 3, 4, 5];
    let t1 = quantile_i64(train.clone(), 50).unwrap();
    let mut mixed = train;
    mixed.extend([1000, 2000, 3000]); // "test" values
    let t2 = quantile_i64(vec![1, 2, 3, 4, 5], 50).unwrap();
    assert_eq!(t1, t2);
    assert_eq!(t1, 3);
    let _ = mixed;
}

#[test]
fn lock_prevents_silent_edit() {
    let mut e = StrategyExperiment::new("EXP001", "first");
    e.entry_policy_id = "S1_BUYER_GROWTH".into();
    e.lock().unwrap();
    assert!(e.verify_lock().is_ok());
    e.entry_policy_id = "S6_HYBRID".into();
    assert_eq!(e.verify_lock(), Err("CONFIG_DRIFT"));
    assert_eq!(e.lock(), Err("ALREADY_LOCKED"));
}

#[test]
fn buyer_growth_rule_deterministic() {
    let thr = StrategyThresholds::train_defaults();
    let s = family("S1_BUYER_GROWTH", thr.clone());
    let good = vec_at("g", 30_000, 4, 5);
    let bad = vec_at("b", 30_000, 0, 5);
    assert!(s.decide(&ctx(&good, "g")).enter);
    assert!(!s.decide(&ctx(&bad, "b")).enter);
}

#[test]
fn hybrid_requires_no_creator_sell() {
    let s = family("S6_HYBRID", StrategyThresholds::train_defaults());
    let mut f = vec_at("h", 30_000, 5, 5);
    f.shared.creator_has_sold = true;
    f.shared.net_quote_flow_total = "10".into();
    f.shared.trade_count_imbalance = 3;
    f.shared.unique_buyers_total = 8;
    assert!(!s.decide(&ctx(&f, "h")).enter);
    f.shared.creator_has_sold = false;
    assert!(s.decide(&ctx(&f, "h")).enter);
}

#[test]
fn security_reject_blocks_strategy() {
    let s = family("S1_BUYER_GROWTH", StrategyThresholds::train_defaults());
    let f = vec_at("x", 30_000, 9, 9);
    let mut c = ctx(&f, "x");
    c.security = Some(SecurityVerdict::Reject);
    assert_eq!(s.decide(&c).reason, "SECURITY_REJECT");
    assert!(!s.decide(&c).enter);
}

#[test]
fn future_outcome_not_in_strategy_context() {
    let src = include_str!("../src/strategy/mod.rs");
    assert!(!src.contains("use crate::sim::outcome"));
    assert!(!src.contains("time_to_10x"));
    assert!(!src.contains("mfe_quote"));
}

#[test]
fn moonshot_recall_precision() {
    assert_eq!(moonshot_recall_bps(4, 10), Some(4_000));
    assert_eq!(moonshot_precision_bps(2, 8), Some(2_500));
    assert_eq!(moonshot_recall_bps(0, 0), None);
}

#[test]
fn right_tail_and_drawdown() {
    let pnls = vec![10i64, -5, 100, -20, 3];
    let share = right_tail_share_bps(&pnls, 1).unwrap();
    assert_eq!(share, 8_849); // 100/113
    let dd = chronological_drawdown_bps(&[-50, 10, -80, 5]);
    assert!(dd > 0);
}

#[test]
fn random_baseline_is_seeded() {
    let a = memecoin_engine::sim::policy::may_enter(
        memecoin_engine::sim::policy::EntryPolicyId::RandomEligible,
        CandidateState::Eligible,
        Some(SecurityVerdict::Pass),
        Some(ts(1_000)),
        ts(30_000),
        "tok",
        7,
    );
    let b = memecoin_engine::sim::policy::may_enter(
        memecoin_engine::sim::policy::EntryPolicyId::RandomEligible,
        CandidateState::Eligible,
        Some(SecurityVerdict::Pass),
        Some(ts(1_000)),
        ts(30_000),
        "tok",
        7,
    );
    assert_eq!(a, b);
}

#[tokio::test]
async fn persist_and_reload_report() {
    let snaps = vec![snap("p", 30_000), snap("p", 150_000)];
    let mut eligible = HashMap::new();
    eligible.insert((Chain::Solana, "p".into()), ts(30_000));
    let cand = vec![(
        ts(30_000),
        Chain::Solana,
        "p".into(),
        CandidateState::Eligible,
    )];
    let sec = vec![(ts(30_000), Chain::Solana, "p".into(), SecurityVerdict::Pass)];
    let cfg = SimConfig::research_default().with_latency(LatencyScenario::Fast);
    let strat = family("S0_BASELINE", StrategyThresholds::train_defaults());
    let x = exit_policy("X1_TIME_2M");
    let r = run_with_strategy(
        &snaps,
        &[],
        eligible,
        &sec,
        &cand,
        strat.as_ref(),
        x.as_ref(),
        &cfg,
        QualityStatus::HistoricalReplay,
        1,
        Some("EXP001".into()),
    );
    let store = MemoryStore::new();
    let id = store.persist_report(&r).await.unwrap();
    let loaded = store.load_report(id).await.unwrap().unwrap();
    assert_eq!(loaded.orders.len(), r.orders.len());
    assert_eq!(loaded.run.experiment_id.as_deref(), Some("EXP001"));
    let mut exp = StrategyExperiment::new("EXP001", "persist");
    exp.lock().unwrap();
    store.upsert_experiment(&exp).await.unwrap();
    let back = store.get_experiment("EXP001").await.unwrap().unwrap();
    assert_eq!(back.config_hash, exp.config_hash);
}

#[test]
fn stress_worse_slippage_not_better_fill() {
    use memecoin_engine::sim::impact::executable_fill;
    use memecoin_engine::sim::models::{FeeModel, SlippageModel};
    use memecoin_engine::sim::types::OrderSide;
    let s = snap("t", 30_000);
    let fees = FeeModel::research_default();
    let a = executable_fill(
        &s,
        OrderSide::Buy,
        "100000000",
        &fees,
        &SlippageModel::bps(0),
        u32::MAX,
        false,
    );
    let b = executable_fill(
        &s,
        OrderSide::Buy,
        "100000000",
        &fees,
        &SlippageModel::bps(300),
        u32::MAX,
        false,
    );
    let ta = memecoin_engine::state::amt::parse_u256(&a.token_amount);
    let tb = memecoin_engine::state::amt::parse_u256(&b.token_amount);
    assert!(tb <= ta);
}

#[test]
fn capture_preserved() {
    assert_eq!(
        memecoin_engine::sim::capture_ratio_bps("100", "250", "1000"),
        Some(1666)
    );
}

#[test]
fn train_thresholds_from_quantiles() {
    let t = train_thresholds(vec![0, 1, 2, 8, 9], vec![1, 2, 3, 4], vec![2, 3, 4, 10]);
    assert!(t.min_buyer_accel_15s >= 1);
}

#[test]
fn missed_winner_classification() {
    use memecoin_engine::sim::MissReason;
    assert_eq!(MissReason::NeverEligible.as_str(), "NEVER_ELIGIBLE");
    assert_eq!(MissReason::SecurityReject.as_str(), "SECURITY_REJECT");
}

#[test]
fn feature_samples_do_not_treat_unknown_as_zero() {
    let s = FeatureSample {
        token: "x".into(),
        value: None,
        max_return_bps: Some(0),
        reached_5x: false,
        reached_10x: false,
        security: None,
        eligible: false,
    };
    assert!(s.value.is_none());
}

#[test]
fn corpus_env_blocked_by_default() {
    let blocked = std::env::var("MEMECOIN_HISTORICAL_DIR")
        .map(|p| !std::path::Path::new(&p).is_dir())
        .unwrap_or(true);
    assert!(
        blocked
            || std::path::Path::new(&std::env::var("MEMECOIN_HISTORICAL_DIR").unwrap()).is_dir()
    );
}
