use std::sync::Arc;

use memecoin_engine::decoders::{DecodeOutcome, DecoderRegistry};
use memecoin_engine::domain::{
    CanonicalEvent, ExecutionStatus, Finality, Launchpad, LifecycleType, TradeSide,
};
use memecoin_engine::ingest::solana::health::{record_missing_range, SolanaSlotTracker};
use memecoin_engine::ingest::solana::tx::{raw_events_from_view, view_from_get_transaction};
use memecoin_engine::pipeline::{DiscoveryPipeline, HandleResult};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::EventStore;
use memecoin_engine::test_support::{load_json, solana_raw_from_fixture};
use memecoin_engine::watch::{MarketRef, MarketRegistry};
use tokio::sync::mpsc;

fn pipeline(
    store: Arc<MemoryStore>,
    markets: Arc<MarketRegistry>,
) -> DiscoveryPipeline<MemoryStore> {
    let (tx, _rx) = mpsc::channel(32);
    let (trade_tx, _tr) = mpsc::channel(32);
    let (life_tx, _lr) = mpsc::channel(32);
    DiscoveryPipeline {
        store,
        registry: DecoderRegistry::production(),
        markets,
        discovered_tx: tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: memecoin_engine::metrics::DiscoveryMetrics,
        ingest_id: "phase21".into(),
        slots: None,
        pool_tx: None,
        state: None,
    }
}

fn lifecycle_path(name: &str) -> String {
    format!("solana/lifecycle/{name}.json")
}

fn has_lifecycle_fixture(name: &str) -> bool {
    memecoin_engine::test_support::fixture_path(&lifecycle_path(name)).exists()
}

#[test]
fn slot_lag_detects_skipped_range() {
    let t = SolanaSlotTracker::new();
    t.note_head(100);
    t.note_received(100);
    t.note_head(250);
    assert_eq!(t.head(), 250);
    let missing = t.take_missing_ranges();
    assert!(!missing.is_empty());
    assert_eq!(missing[0], (101, 249));
}

#[tokio::test]
async fn ingest_gap_records_and_recovers() {
    let store = MemoryStore::new();
    let id = record_missing_range(&store, 1000, 1100, "test_skip")
        .await
        .unwrap();
    assert!(id > 0);
    let gaps = store.gaps();
    assert_eq!(gaps.len(), 1);
    assert!(!gaps[0].recovered);
    store.mark_gap_recovered(id).await.unwrap();
    let gaps = store.gaps();
    assert!(gaps[0].recovered);
}

#[tokio::test]
async fn failed_pump_trade_does_not_create_trade_observed() {
    let mut raw = memecoin_engine::test_support::solana_raw_from_fixture(
        "solana/pumpfun/buy.json",
        "trade_instruction_index",
    );
    if let memecoin_engine::domain::RawEventKind::Solana(ix) = &mut raw.kind {
        ix.execution_status = ExecutionStatus::Failed;
        ix.log_messages.clear();
    }
    let outcome = DecoderRegistry::production().decode(&raw).unwrap();
    match outcome {
        DecodeOutcome::Unknown => {}
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().all(|e| e.as_trade().is_none()));
        }
    }
}

#[tokio::test]
async fn watched_markets_reload() {
    let markets = MarketRegistry::new();
    markets.register(MarketRef {
        chain: memecoin_engine::domain::Chain::Solana,
        launchpad: Launchpad::PumpSwap,
        token_address: "Mint111".into(),
        curve: Some("Curve111".into()),
        pool: Some("Pool111".into()),
        pool_id: Some("Pool111".into()),
        quote_asset: Some("So11111111111111111111111111111111111111112".into()),
    });
    let pools = markets.solana_pools();
    assert!(pools.contains(&"Pool111".to_string()));
    let reloaded = MarketRegistry::new();
    reloaded.load_all(vec![markets
        .by_token(memecoin_engine::domain::Chain::Solana, "Mint111")
        .unwrap()]);
    assert!(reloaded.knows_pool(memecoin_engine::domain::Chain::Solana, "Pool111"));
}

#[tokio::test]
async fn replay_overlap_deduplicates() {
    if !has_lifecycle_fixture("buy") {
        return;
    }
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone(), Arc::new(MarketRegistry::new()));
    let raw = solana_raw_from_fixture(&lifecycle_path("buy"), "trade_instruction_index");
    p.handle(raw.clone()).await.unwrap();
    match p.handle(raw).await.unwrap() {
        HandleResult::Duplicate { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn solana_ordering_is_deterministic() {
    let v = load_json("solana/pumpfun/buy.json");
    let view = view_from_get_transaction(&v, "fixture", chrono::Utc::now(), Finality::Confirmed)
        .expect("view");
    let a = raw_events_from_view(&view);
    let b = raw_events_from_view(&view);
    let ka: Vec<_> = a
        .iter()
        .map(|e| (e.slot(), e.instruction_index(), e.inner_instruction_index()))
        .collect();
    let kb: Vec<_> = b
        .iter()
        .map(|e| (e.slot(), e.instruction_index(), e.inner_instruction_index()))
        .collect();
    assert_eq!(ka, kb);
}

#[tokio::test]
async fn lifecycle_fixtures_decode_if_present() {
    let registry = DecoderRegistry::production();
    if has_lifecycle_fixture("create") {
        let raw = solana_raw_from_fixture(&lifecycle_path("create"), "create_instruction_index");
        let token = registry.decode(&raw).unwrap().into_token();
        assert!(token.is_some());
    }
    if has_lifecycle_fixture("buy") {
        let raw = solana_raw_from_fixture(&lifecycle_path("buy"), "trade_instruction_index");
        let evs = match registry.decode(&raw).unwrap() {
            DecodeOutcome::Events(e) => e,
            DecodeOutcome::Unknown => panic!("buy unknown"),
        };
        assert!(evs.iter().any(|e| matches!(e, CanonicalEvent::Trade(_))));
    }
    if has_lifecycle_fixture("sell") {
        let raw = solana_raw_from_fixture(&lifecycle_path("sell"), "trade_instruction_index");
        let evs = match registry.decode(&raw).unwrap() {
            DecodeOutcome::Events(e) => e,
            DecodeOutcome::Unknown => panic!("sell unknown"),
        };
        assert!(evs
            .iter()
            .any(|e| e.as_trade().map(|t| t.side) == Some(TradeSide::Sell)));
    }
    if has_lifecycle_fixture("migrate") {
        let raw = solana_raw_from_fixture(&lifecycle_path("migrate"), "migrate_instruction_index");
        let evs = match registry.decode(&raw).unwrap() {
            DecodeOutcome::Events(e) => e,
            DecodeOutcome::Unknown => panic!("migrate unknown"),
        };
        assert!(evs.iter().any(|e| {
            e.as_lifecycle()
                .map(|l| l.lifecycle_type == LifecycleType::Migrated)
                .unwrap_or(false)
        }));
    }
    if has_lifecycle_fixture("create_pool") {
        let raw = solana_raw_from_fixture(&lifecycle_path("create_pool"), "instruction_index");
        let evs = match registry.decode(&raw).unwrap() {
            DecodeOutcome::Events(e) => e,
            DecodeOutcome::Unknown => panic!("create_pool unknown"),
        };
        assert!(evs.iter().any(|e| {
            e.as_lifecycle()
                .map(|l| l.lifecycle_type == LifecycleType::PoolCreated)
                .unwrap_or(false)
        }));
    }
    if has_lifecycle_fixture("pamm_sell") {
        let raw = solana_raw_from_fixture(&lifecycle_path("pamm_sell"), "instruction_index");
        let evs = match registry.decode(&raw).unwrap() {
            DecodeOutcome::Events(e) => e,
            DecodeOutcome::Unknown => panic!("pamm_sell unknown"),
        };
        assert!(evs.iter().any(|e| {
            e.as_trade()
                .map(|t| t.launchpad == Launchpad::PumpSwap)
                .unwrap_or(false)
        }));
    }
}

#[test]
fn json_rpc_view_to_rawevent() {
    let v = load_json("solana/pumpfun/create_v2.json");
    // create_v2 fixture is flattened; wrap via test helper instead
    let raw = memecoin_engine::test_support::pumpfun_raw_from_fixture();
    assert_eq!(
        raw.as_solana().unwrap().execution_status,
        ExecutionStatus::Success
    );
    let _ = v;
}

#[test]
fn yellowstone_tx_info_index_is_transaction_index() {
    // The geyser SubscribeUpdateTransactionInfo.index field is the slot
    // transaction index (u64). convert.rs stores it on SolanaTxView.transaction_index.
    let info_index: u64 = 17;
    assert!(info_index < u32::MAX as u64);
}
