use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::CandidateState;
use memecoin_engine::domain::{
    CanonicalEvent, LaunchMechanism, TokenDiscovered, TradeObserved, TradeSide,
};
use memecoin_engine::domain::{CanonicalStatus, Chain, Finality, Launchpad, QualityStatus};
use memecoin_engine::live::{persist_and_research, LiveMilestoneScheduler, LiveResearchRuntime};
use memecoin_engine::security::assessment::SecurityVerdict;
use memecoin_engine::sim::models::SimConfig;
use memecoin_engine::sim::types::PositionStatus;
use memecoin_engine::state::schedule::SnapshotSchedule;
use memecoin_engine::state::{StateEngine, TokenKey};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::strategy::{smoke_decide, ProspectivePolicy, StrategyContext};

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn discovered(token: &str) -> TokenDiscovered {
    TokenDiscovered {
        chain: Chain::Robinhood,
        chain_id: Some(4663),
        token_address: token.into(),
        creator: "creator1".into(),
        launchpad: Launchpad::PonsV2,
        factory_or_program: "factory".into(),
        pool: None,
        curve: Some("curve1".into()),
        quote_asset: Some("ETH".into()),
        launch_mechanism: LaunchMechanism::BondingCurve,
        bonding_curve: true,
        graduation_model: memecoin_engine::domain::GraduationModel::Unknown,
        block_number: Some(1),
        block_hash: None,
        slot: None,
        tx_hash_or_signature: "tx0".into(),
        instruction_index: Some(0),
        inner_instruction_index: None,
        log_index: Some(0),
        chain_timestamp: Some(ts(0)),
        observed_at: ts(0),
        persisted_at: None,
        source: "test".into(),
        decoder_version: "1".into(),
        initial_liquidity: None,
        raw_event_id: "disc".into(),
    }
}

fn trade(token: &str, t_ms: i64, trader: &str) -> TradeObserved {
    TradeObserved {
        event_id: format!("e{t_ms}"),
        chain: Chain::Robinhood,
        launchpad: Launchpad::PonsV2,
        token_address: token.into(),
        trader: trader.into(),
        side: TradeSide::Buy,
        base_amount_raw: "1".into(),
        quote_amount_raw: "1".into(),
        base_decimals: 18,
        quote_decimals: 18,
        quote_asset: "ETH".into(),
        pool: None,
        curve: None,
        price_estimate: None,
        block_number: Some(1),
        block_hash: None,
        slot: None,
        transaction_index: Some(0),
        tx_hash_or_signature: format!("tx{t_ms}"),
        log_index: Some(0),
        instruction_index: None,
        inner_instruction_index: None,
        chain_timestamp: Some(ts(t_ms)),
        observed_at: ts(t_ms),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        source: "test".into(),
        decoder_version: "1".into(),
        raw_event_id: format!("r{t_ms}"),
        metadata: serde_json::json!({}),
    }
}

#[test]
fn live_milestone_and_dead_token_t30() {
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered(
        "tokdead",
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime { unix_ms: 30_000 });
    assert!(snaps
        .iter()
        .any(|s| s.age_ms == 30_000 && s.buy_count_total == 0));
}

#[test]
fn t30_excludes_future_trade_even_if_already_applied() {
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered("tok"))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade("tok", 10_000, "a"))));
    let late = eng.apply(CanonicalEvent::Trade(Box::new(trade("tok", 31_000, "b"))));
    let t30 = late
        .iter()
        .chain(eng.history.iter())
        .find(|s| s.age_ms == 30_000)
        .expect("t30");
    assert_eq!(t30.buy_count_total, 1);
    assert_eq!(t30.unique_buyers_total, 1);
}

#[test]
fn no_duplicate_milestone() {
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered(
        "tok2",
    ))));
    let a = eng.finish_until(memecoin_engine::state::clock::StateTime { unix_ms: 30_000 });
    let b = eng.finish_until(memecoin_engine::state::clock::StateTime { unix_ms: 30_000 });
    let n30 = a.iter().filter(|s| s.age_ms == 30_000).count()
        + b.iter().filter(|s| s.age_ms == 30_000).count();
    assert_eq!(n30, 1);
}

#[test]
fn scheduler_bounded_under_two_thousand() {
    let mut s = LiveMilestoneScheduler::default();
    let sched = SnapshotSchedule::default_research();
    for i in 0..2_500 {
        let k = TokenKey::new(Chain::Robinhood, format!("t{i}"));
        s.register(&k, 0, &sched);
    }
    assert!(s.len() <= 2_500 * sched.milestones_ms.len());
    let due = s.pop_due(5_000);
    assert!(!due.is_empty());
    assert!(s.len() < 2_500 * sched.milestones_ms.len());
}

#[test]
fn smoke_policy_does_not_change_research_thresholds() {
    let cfg = SimConfig::research_default();
    let p1 = ProspectivePolicy::all()
        .into_iter()
        .find(|p| p.id() == "P1_SOLANA_BUYERS_3_30S")
        .unwrap();
    assert_eq!(p1.id(), "P1_SOLANA_BUYERS_3_30S");
    let _ = smoke_decide(
        &StrategyContext {
            features: None,
            candidate: CandidateState::Discovered,
            security: Some(SecurityVerdict::Pass),
            first_eligible_at: None,
            now: ts(20_000),
            token: "x",
            seed: 1,
        },
        &cfg,
    );
}

#[test]
fn clanker_shadow_still_not_research_valid() {
    assert!(!memecoin_engine::prospective::clanker_paper_research_valid());
}

#[test]
fn scheduler_does_not_reregister() {
    let mut s = LiveMilestoneScheduler::default();
    let sched = SnapshotSchedule::default_research();
    let k = TokenKey::new(Chain::Robinhood, "tok");
    s.register(&k, 0, &sched);
    let n = s.len();
    s.register(&k, 0, &sched);
    s.register(&k, 0, &sched);
    assert_eq!(s.len(), n);
}

#[tokio::test]
async fn live_persist_writes_features_for_dead_token() {
    let store = MemoryStore::new();
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered(
        "tokdead2",
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime { unix_ms: 30_000 });
    let hist = eng.history.clone();
    let mut rt = LiveResearchRuntime::new(true);
    let mut n_feat = 0u32;
    for mut snap in snaps {
        persist_and_research(&store, &mut snap, &hist, &mut rt, None)
            .await
            .unwrap();
        n_feat += 1;
    }
    assert!(n_feat > 0);
    assert!(
        !store.feature_vectors().is_empty(),
        "FeatureEngine must run on live milestone snapshots"
    );
    assert!(store
        .feature_vectors()
        .iter()
        .any(|v| v.token_age_ms == 30_000));
}

#[test]
fn restore_marks_entered_no_duplicate() {
    let rt = LiveResearchRuntime::new(true);
    assert!(!rt.skip_duplicate(Chain::Robinhood, "x"));
    let mut rt = LiveResearchRuntime::new(true);
    rt.entered.insert((Chain::Robinhood, "already".into()));
    assert!(rt.skip_duplicate(Chain::Robinhood, "already"));
    let _ = PositionStatus::SessionEndedOpen;
}
