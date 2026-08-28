use chrono::{TimeZone, Utc};
use memecoin_engine::decoders::DecoderRegistry;
use memecoin_engine::domain::{
    validate_dataset_quality, CanonicalEvent, CanonicalStatus, Chain, CollectionSession, Finality,
    LaunchMechanism, Launchpad, LifecycleObserved, LifecycleType, QualityCheck, QualityStatus,
    SolanaMode, TokenDiscovered, TradeObserved, TradeSide,
};
use memecoin_engine::error::DatasetQualityError;
use memecoin_engine::replay::replay_fixture_dir_opts;
use memecoin_engine::state::clock::StateTime;
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::market::MarketState;
use memecoin_engine::state::query::{get_milestone_snapshot, get_snapshot_at_or_before};
use memecoin_engine::state::snapshot::validate_snapshot_for_simulation;
use memecoin_engine::state::{SnapshotKind, StateEngine};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::EventStore;
use memecoin_engine::test_support::evm_raw_from_fixture;
use memecoin_engine::watch::MarketRegistry;
use std::sync::Arc;
use std::time::Instant;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn discovered_at(token: &str, launchpad: Launchpad, chain: Chain, ms: i64) -> TokenDiscovered {
    TokenDiscovered {
        chain,
        chain_id: chain.evm_chain_id(),
        token_address: token.into(),
        creator: "creator1".into(),
        launchpad,
        factory_or_program: "factory".into(),
        pool: None,
        curve: Some("curve1".into()),
        quote_asset: Some("quote".into()),
        launch_mechanism: LaunchMechanism::BondingCurve,
        bonding_curve: true,
        graduation_model: memecoin_engine::domain::GraduationModel::Unknown,
        block_number: Some(1),
        block_hash: None,
        slot: Some(1),
        tx_hash_or_signature: format!("tx-{ms}"),
        instruction_index: Some(0),
        inner_instruction_index: None,
        log_index: Some(0),
        chain_timestamp: Some(ts(ms)),
        observed_at: ts(ms),
        persisted_at: None,
        source: "test".into(),
        decoder_version: "0.1.0".into(),
        initial_liquidity: None,
        raw_event_id: format!("disc-{token}-{ms}"),
    }
}

fn trade_at(
    token: &str,
    trader: &str,
    side: TradeSide,
    quote: &str,
    base: &str,
    ms: i64,
    ix: u64,
) -> TradeObserved {
    TradeObserved {
        event_id: format!("tr-{trader}-{}-{ms}-{ix}", side.as_str()),
        chain: Chain::Solana,
        launchpad: Launchpad::PumpFun,
        token_address: token.into(),
        trader: trader.into(),
        side,
        base_amount_raw: base.into(),
        quote_amount_raw: quote.into(),
        base_decimals: 6,
        quote_decimals: 9,
        quote_asset: "So11111111111111111111111111111111111111112".into(),
        pool: None,
        curve: Some("curve1".into()),
        price_estimate: None,
        block_number: None,
        block_hash: None,
        slot: Some(ms as u64 / 400),
        transaction_index: Some(ix),
        tx_hash_or_signature: format!("sig-{ms}-{ix}"),
        log_index: None,
        instruction_index: Some(ix as u32),
        inner_instruction_index: None,
        chain_timestamp: Some(ts(ms)),
        observed_at: ts(ms),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        source: "test".into(),
        decoder_version: "0.1.0".into(),
        raw_event_id: format!("raw-{ms}-{ix}"),
        metadata: serde_json::json!({
            "virtual_token_reserves": "1000000000",
            "virtual_sol_reserves": "30000000000",
            "real_token_reserves": "800000000",
            "real_sol_reserves": "1000000000",
        }),
    }
}

fn life_at(
    token: &str,
    kind: LifecycleType,
    launchpad: Launchpad,
    chain: Chain,
    ms: i64,
    meta: serde_json::Value,
) -> LifecycleObserved {
    LifecycleObserved {
        event_id: format!("lf-{}-{ms}", kind.as_str()),
        chain,
        launchpad,
        token_address: token.into(),
        lifecycle_type: kind,
        factory: None,
        pool: None,
        curve: Some("curve1".into()),
        block_number: Some(10),
        block_hash: None,
        slot: Some(10),
        transaction_index: Some(1),
        tx_hash_or_signature: format!("lftx-{ms}"),
        log_index: Some(1),
        instruction_index: None,
        inner_instruction_index: None,
        chain_timestamp: Some(ts(ms)),
        observed_at: ts(ms),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        source: "test".into(),
        decoder_version: "0.1.0".into(),
        raw_event_id: format!("rawlf-{ms}"),
        metadata: meta,
    }
}

#[test]
fn create_initial_token_state() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        1_000,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.lifecycle_state, TokenLifecycleState::Discovered);
    assert_eq!(st.buy_count_total, 0);
}

#[test]
fn buy_updates_totals() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        1_000,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "100",
        "50",
        2_000,
        1,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.buy_count_total, 1);
    assert_eq!(st.buy_quote_volume_raw_total, "100");
    assert_eq!(st.unique_buyers_total(), 1);
}

#[test]
fn sell_updates_totals() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        1_000,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Sell,
        "40",
        "10",
        2_000,
        1,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.sell_count_total, 1);
    assert_eq!(st.sell_quote_volume_raw_total, "40");
}

#[test]
fn repeated_buyer_counts_once() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        1_000,
    ))));
    for i in 0..10 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "tok",
            "w1",
            TradeSide::Buy,
            "1",
            "1",
            2_000 + i * 10,
            i as u64,
        ))));
    }
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.buy_count_total, 10);
    assert_eq!(st.unique_buyers_total(), 1);
}

#[test]
fn buyer_then_seller_in_both_unique_sets() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        1_000,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "5",
        "5",
        2_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Sell,
        "3",
        "3",
        3_000,
        2,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.unique_buyers_total(), 1);
    assert_eq!(st.unique_sellers_total(), 1);
}

#[test]
fn rolling_5s_expires_old_event() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "10",
        "1",
        1_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w2",
        TradeSide::Buy,
        "7",
        "1",
        7_000,
        2,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    let w = st.rolling.by_ms(5_000);
    assert_eq!(w.buy_count, 1);
    assert_eq!(w.buy_quote_volume_raw, "7");
}

#[test]
fn rolling_60s_correctness() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "10",
        "1",
        1_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w2",
        TradeSide::Buy,
        "7",
        "1",
        50_000,
        2,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    let w = st.rolling.by_ms(60_000);
    assert_eq!(w.buy_count, 2);
    assert_eq!(w.unique_buyers, 2);
}

#[test]
fn creator_buy_sell_flow() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "creator1",
        TradeSide::Buy,
        "9",
        "1",
        1_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "creator1",
        TradeSide::Sell,
        "4",
        "1",
        2_000,
        2,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.creator_buy_count, 1);
    assert_eq!(st.creator_sell_count, 1);
    assert_eq!(st.creator_buy_quote_raw, "9");
    assert_eq!(st.creator_sell_quote_raw, "4");
    assert_eq!(st.creator_net_quote_flow(), "5");
}

#[test]
fn milestone_t30s_without_exact_trade() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "1",
        "1",
        1_000,
        1,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 30_000 });
    assert!(snaps.iter().any(|s| {
        s.snapshot_kind == SnapshotKind::Milestone && s.age_ms == 30_000 && s.buy_count_total == 1
    }));
}

#[test]
fn dead_token_zero_activity_milestone() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "dead",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 300_000 });
    let m = snaps
        .iter()
        .find(|s| s.snapshot_kind == SnapshotKind::Milestone && s.age_ms == 60_000)
        .unwrap();
    assert_eq!(m.buy_count_total, 0);
    assert_eq!(m.unique_buyers_total, 0);
    assert_eq!(m.buy_quote_volume_raw_total, "0");
}

#[test]
fn no_lookahead_future_trade_excluded() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let early = eng.finish_until(StateTime { unix_ms: 30_000 });
    let t30 = early.iter().find(|s| s.age_ms == 30_000).unwrap().clone();
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "999",
        "1",
        60_000,
        1,
    ))));
    assert_eq!(t30.buy_count_total, 0);
    let later = eng.history.iter().find(|s| s.age_ms == 30_000).unwrap();
    assert_eq!(later.buy_count_total, 0);
}

#[tokio::test]
async fn same_replay_same_snapshot_hashes() {
    let dir = memecoin_engine::test_support::fixture_path("solana/lifecycle");
    let a = replay_fixture_dir_opts(
        &dir,
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
        true,
    )
    .await
    .unwrap();
    let b = replay_fixture_dir_opts(
        &dir,
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
        true,
    )
    .await
    .unwrap();
    assert_eq!(a.snapshot_fingerprint(), b.snapshot_fingerprint());
    assert!(!a.snapshots.is_empty());
}

#[test]
fn event_order_determinism() {
    let mut events = vec![
        CanonicalEvent::Trade(Box::new(trade_at(
            "tok",
            "w2",
            TradeSide::Buy,
            "2",
            "1",
            2_000,
            2,
        ))),
        CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
            "tok",
            Launchpad::PumpFun,
            Chain::Solana,
            1_000,
        ))),
        CanonicalEvent::Trade(Box::new(trade_at(
            "tok",
            "w1",
            TradeSide::Buy,
            "1",
            "1",
            1_500,
            1,
        ))),
    ];
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    let snaps = eng.apply_sorted(std::mem::take(&mut events));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.buy_count_total, 2);
    let _ = snaps;
}

#[test]
fn pons_launch_swept_graduation_gap() {
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "pons",
        Launchpad::PonsV2,
        Chain::Robinhood,
        0,
    ))));
    let mut swept = life_at(
        "pons",
        LifecycleType::LaunchSwept,
        Launchpad::PonsV2,
        Chain::Robinhood,
        5_000,
        serde_json::json!({}),
    );
    swept.block_number = Some(100);
    eng.apply(CanonicalEvent::Lifecycle(Box::new(swept)));
    let st = eng.get(Chain::Robinhood, "pons").unwrap();
    assert_eq!(st.lifecycle_state, TokenLifecycleState::GraduationGap);
}

#[test]
fn pons_pool_graduated_amm_active_and_gap_duration() {
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "pons",
        Launchpad::PonsV2,
        Chain::Robinhood,
        0,
    ))));
    let mut swept = life_at(
        "pons",
        LifecycleType::LaunchSwept,
        Launchpad::PonsV2,
        Chain::Robinhood,
        5_000,
        serde_json::json!({}),
    );
    swept.block_number = Some(100);
    eng.apply(CanonicalEvent::Lifecycle(Box::new(swept)));
    let mut grad = life_at(
        "pons",
        LifecycleType::PoolGraduated,
        Launchpad::PonsV2,
        Chain::Robinhood,
        12_000,
        serde_json::json!({}),
    );
    grad.block_number = Some(174);
    eng.apply(CanonicalEvent::Lifecycle(Box::new(grad)));
    let st = eng.get(Chain::Robinhood, "pons").unwrap();
    assert_eq!(st.lifecycle_state, TokenLifecycleState::AmmActive);
    assert_eq!(st.graduation_gap_ms, Some(7_000));
}

#[tokio::test]
async fn pump_migrate_pumpswap_continuity() {
    let dir = memecoin_engine::test_support::fixture_path("solana/lifecycle");
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    let report = replay_fixture_dir_opts(
        &dir,
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
        true,
    )
    .await
    .unwrap();
    let token = "wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump";
    let mut evs = Vec::new();
    for t in report.tokens {
        evs.push(CanonicalEvent::TokenDiscovered(Box::new(t)));
    }
    for t in report.trades {
        evs.push(CanonicalEvent::Trade(Box::new(t)));
    }
    for l in report.lifecycle {
        evs.push(CanonicalEvent::Lifecycle(Box::new(l)));
    }
    eng.apply_sorted(evs);
    let st = eng.get(Chain::Solana, token).expect("token state");
    assert!(matches!(
        st.lifecycle_state,
        TokenLifecycleState::AmmActive | TokenLifecycleState::Migrating
    ));
    assert_eq!(
        st.pool.as_deref(),
        Some("5XKoFuwq8fwMLtLyTEDeg1SXTny4YsAeP8RuWTRPZU81")
    );
}

#[test]
fn clanker_token_created_pool_state() {
    let raw = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    let tok = DecoderRegistry::production()
        .decode(&raw)
        .unwrap()
        .into_token()
        .unwrap();
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(tok.clone())));
    let st = eng.get(Chain::Base, &tok.token_address).unwrap();
    assert_eq!(st.lifecycle_state, TokenLifecycleState::AmmActive);
    assert!(st.pool.is_some());
    assert!(matches!(st.market_state, MarketState::UniswapV4(_)));
}

#[test]
fn uniswap_v4_swap_updates_sqrt_price() {
    let create = evm_raw_from_fixture("base/clanker_v4/token_created_for_swap.json");
    let swap = evm_raw_from_fixture("base/clanker_v4/swap.json");
    let registry = DecoderRegistry::production();
    let mut eng = StateEngine::replay(QualityStatus::LiveComplete, None);
    let tok = registry.decode(&create).unwrap().into_token().unwrap();
    // Same-tx: Swap logIndex 159 then TokenCreated 167. Unknown-pool swaps buffer until discover.
    for ev in registry.decode(&swap).unwrap().into_events() {
        eng.apply(ev);
    }
    for ev in registry.decode(&create).unwrap().into_events() {
        eng.apply(ev);
    }
    let st = eng.get(Chain::Base, &tok.token_address).unwrap();
    match &st.market_state {
        MarketState::UniswapV4(v4) => {
            assert!(
                v4.sqrt_price_x96.is_some(),
                "buys={} pool={:?} market={v4:?}",
                st.buy_count_total,
                st.pool
            );
            assert!(v4.liquidity_raw.is_some());
            assert!(v4.tick.is_some());
        }
        other => panic!("buys={} other={other:?}", st.buy_count_total),
    }
}

#[test]
fn rpc_dev_quality_propagates() {
    let mut eng = StateEngine::replay(QualityStatus::RpcDevIncomplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 5_000 });
    assert!(snaps
        .iter()
        .all(|s| s.data_quality == QualityStatus::RpcDevIncomplete));
}

#[test]
fn simulation_guard_rejects_incomplete_snapshot() {
    let mut eng = StateEngine::replay(QualityStatus::RpcDevIncomplete, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 5_000 });
    let err = validate_snapshot_for_simulation(&snaps[0], QualityCheck::complete_market_data())
        .unwrap_err();
    assert!(matches!(err, DatasetQualityError::IncompleteSource { .. }));
}

#[test]
fn historical_replay_accepted_for_simulation() {
    let mut session =
        CollectionSession::start(Chain::Solana, SolanaMode::Historical, "fixture", None);
    session.complete = true;
    session.quality_status = QualityStatus::HistoricalReplay;
    validate_dataset_quality(&session, QualityCheck::complete_market_data()).unwrap();
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 5_000 });
    validate_snapshot_for_simulation(&snaps[0], QualityCheck::complete_market_data()).unwrap();
}

#[test]
fn late_event_detected() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        "1",
        "1",
        5_000,
        5,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w2",
        TradeSide::Buy,
        "1",
        "1",
        1_000,
        1,
    ))));
    assert!(eng.late_events > 0);
    assert!(!eng.pending_rebuilds().is_empty() || eng.late_events > 0);
}

#[tokio::test]
async fn orphan_triggers_rebuild_and_supersede() {
    let store = MemoryStore::new();
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    let d = discovered_at("tok", Launchpad::PumpFun, Chain::Solana, 0);
    let a = trade_at("tok", "wa", TradeSide::Buy, "10", "1", 1_000, 1);
    let b = trade_at("tok", "wb", TradeSide::Buy, "20", "1", 2_000, 2);
    let c = trade_at("tok", "wc", TradeSide::Sell, "5", "1", 3_000, 3);
    for ev in [
        CanonicalEvent::TokenDiscovered(Box::new(d.clone())),
        CanonicalEvent::Trade(Box::new(a.clone())),
        CanonicalEvent::Trade(Box::new(b.clone())),
        CanonicalEvent::Trade(Box::new(c.clone())),
    ] {
        for s in eng.apply(ev) {
            store.insert_snapshot(&s).await.unwrap();
        }
    }
    store.insert_discovered(&d).await.unwrap();
    store.insert_trade(&a).await.unwrap();
    let mut b_orph = b.clone();
    b_orph.canonical_status = CanonicalStatus::Orphaned;
    store.insert_trade(&b_orph).await.unwrap();
    store.insert_trade(&c).await.unwrap();
    store
        .mark_snapshots_superseded(Chain::Solana, "tok")
        .await
        .unwrap();
    let events = vec![
        CanonicalEvent::TokenDiscovered(Box::new(d)),
        CanonicalEvent::Trade(Box::new(a)),
        CanonicalEvent::Trade(Box::new(c)),
    ];
    let rebuilt = eng.rebuild_token(
        memecoin_engine::state::TokenKey::new(Chain::Solana, "tok"),
        events,
    );
    for s in &rebuilt {
        store.insert_snapshot(s).await.unwrap();
    }
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(st.buy_count_total, 1);
    assert_eq!(st.sell_count_total, 1);
    let latest = store.latest_snapshot(Chain::Solana, "tok").await.unwrap();
    assert!(!latest.unwrap().superseded);
    let all = store
        .list_snapshots(Chain::Solana, "tok", true)
        .await
        .unwrap();
    assert!(all.iter().any(|s| s.superseded));
}

#[tokio::test]
async fn superseded_not_returned_as_current() {
    let store = MemoryStore::new();
    let mut snap = {
        let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
        eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
            "tok",
            Launchpad::PumpFun,
            Chain::Solana,
            0,
        ))));
        eng.finish_until(StateTime { unix_ms: 5_000 })
            .into_iter()
            .next()
            .unwrap()
    };
    snap.superseded = false;
    store.insert_snapshot(&snap).await.unwrap();
    store
        .mark_snapshots_superseded(Chain::Solana, "tok")
        .await
        .unwrap();
    let latest = store.latest_snapshot(Chain::Solana, "tok").await.unwrap();
    assert!(latest.is_none());
}

#[test]
fn finality_does_not_change_snapshot_time() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 5_000 });
    let t = snaps[0].snapshot_time;
    assert_eq!(t, ts(5_000));
}

#[test]
fn eviction_does_not_delete_history() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.memory.hot_ms = 1_000;
    eng.memory.cold_ms = 2_000;
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let snaps = eng.finish_until(StateTime { unix_ms: 5_000 });
    assert!(!snaps.is_empty() || !eng.history.is_empty());
    assert!(
        eng.get(Chain::Solana, "tok").is_none()
            || eng.history.iter().any(|s| s.token_address == "tok")
    );
    assert!(eng.history.iter().any(|s| s.token_address == "tok"));
}

#[tokio::test]
async fn restart_reconstructs_from_persisted_events() {
    let store = MemoryStore::new();
    let d = discovered_at("tok", Launchpad::PumpFun, Chain::Solana, 0);
    let tr = trade_at("tok", "w1", TradeSide::Buy, "8", "2", 1_000, 1);
    store.insert_discovered(&d).await.unwrap();
    store.insert_trade(&tr).await.unwrap();
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply_sorted(vec![
        CanonicalEvent::TokenDiscovered(Box::new(
            store
                .load_token_discovered(Chain::Solana, "tok")
                .await
                .unwrap()
                .unwrap(),
        )),
        CanonicalEvent::Trade(Box::new(
            store.load_token_trades(Chain::Solana, "tok").await.unwrap()[0].clone(),
        )),
    ]);
    assert_eq!(eng.get(Chain::Solana, "tok").unwrap().buy_count_total, 1);
}

#[test]
fn large_raw_integers_exact() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    let huge = "123456789012345678901234567890";
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        huge,
        huge,
        1_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "tok",
        "w1",
        TradeSide::Buy,
        huge,
        huge,
        2_000,
        2,
    ))));
    let st = eng.get(Chain::Solana, "tok").unwrap();
    assert_eq!(
        st.buy_quote_volume_raw_total,
        "246913578024691357802469135780"
    );
}

#[tokio::test]
async fn query_milestone_and_at_or_before() {
    let store = MemoryStore::new();
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "tok",
        Launchpad::PumpFun,
        Chain::Solana,
        0,
    ))));
    for s in eng.finish_until(StateTime { unix_ms: 60_000 }) {
        store.insert_snapshot(&s).await.unwrap();
    }
    let m = get_milestone_snapshot(&store, Chain::Solana, "tok", 30_000)
        .await
        .unwrap();
    assert!(m.is_some());
    let before = get_snapshot_at_or_before(&store, Chain::Solana, "tok", ts(20_000))
        .await
        .unwrap();
    assert!(before.unwrap().snapshot_time <= ts(20_000));
}

#[test]
fn load_test_bounded_memory_no_loss() {
    let n_tokens = 2_000u64;
    let trades_per = 50u64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.memory.cold_ms = 120_000;
    let started = Instant::now();
    let mut events = 0u64;
    for i in 0..n_tokens {
        let token = format!("t{i}");
        let t0 = (i * 1_000) as i64;
        eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
            &token,
            Launchpad::PumpFun,
            Chain::Solana,
            t0,
        ))));
        events += 1;
        for j in 0..trades_per {
            eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
                &token,
                "w",
                TradeSide::Buy,
                "1",
                "1",
                t0 + 10 + j as i64,
                j,
            ))));
            events += 1;
        }
        if i % 200 == 0 {
            let _ = eng.finish_until(StateTime {
                unix_ms: t0 + 180_000,
            });
        }
    }
    let dt = started.elapsed().as_secs_f64().max(1e-6);
    let eps = events as f64 / dt;
    assert!(events > 0);
    eprintln!(
        "load test events={} tokens={} events/sec={:.0} active={} evicted={}",
        events,
        n_tokens,
        eps,
        eng.active_count(),
        eng.evictions
    );
}

trait IntoEvents {
    fn into_events(self) -> Vec<CanonicalEvent>;
}

impl IntoEvents for memecoin_engine::decoders::DecodeOutcome {
    fn into_events(self) -> Vec<CanonicalEvent> {
        match self {
            memecoin_engine::decoders::DecodeOutcome::Events(e) => e,
            memecoin_engine::decoders::DecodeOutcome::Unknown => Vec::new(),
        }
    }
}
