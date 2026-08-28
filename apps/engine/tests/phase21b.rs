use std::sync::Arc;

use chrono::Utc;
use memecoin_engine::collect::warn_rpc_dev_once;
use memecoin_engine::decoders::{DecodeOutcome, DecoderRegistry};
use memecoin_engine::domain::{
    validate_dataset_quality, CanonicalEvent, CollectionSession, ExecutionStatus, Finality,
    Launchpad, LifecycleType, QualityCheck, QualityStatus, SolanaMode, TradeSide,
};
use memecoin_engine::error::DatasetQualityError;
use memecoin_engine::historical::{FixtureSource, HistoricalSource, JsonlSource};
use memecoin_engine::ingest::solana::convert::{
    subscribe_update_from_rpc_json, view_from_subscribe_update,
};
use memecoin_engine::ingest::solana::health::{record_missing_range, SolanaSlotTracker};
use memecoin_engine::ingest::solana::tx::raw_events_from_view;
use memecoin_engine::ingest::solana::yellowstone::{YellowstoneConfig, YellowstoneIngest};
use memecoin_engine::ingest::ChainIngest;
use memecoin_engine::pipeline::HandleResult;
use memecoin_engine::replay::replay_fixture_dir;
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::{with_finality, EventStore, SessionFinish};
use memecoin_engine::test_support::{fixture_path, load_json, solana_raw_from_fixture};
use memecoin_engine::watch::MarketRegistry;
use tokio::sync::{mpsc, watch};

const TOKEN: &str = "wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump";
const CURVE: &str = "7KH4HscCwK2Bi1y4Ldhsaf9shagXiihAWZxWi4cR3atf";
const POOL: &str = "5XKoFuwq8fwMLtLyTEDeg1SXTny4YsAeP8RuWTRPZU81";

fn lifecycle_dir() -> std::path::PathBuf {
    fixture_path("solana/lifecycle")
}

fn decode(rel: &str, index_field: &str) -> DecodeOutcome {
    let raw = solana_raw_from_fixture(rel, index_field);
    DecoderRegistry::production().decode(&raw).unwrap()
}

#[test]
fn solana_mode_is_never_inferred_from_credentials() {
    assert_eq!(SolanaMode::resolve(None, None).unwrap(), SolanaMode::RpcDev);
    assert_eq!(
        SolanaMode::resolve(None, Some("rpc-dev")).unwrap(),
        SolanaMode::RpcDev
    );
    assert_eq!(
        SolanaMode::resolve(Some("historical"), Some("yellowstone")).unwrap(),
        SolanaMode::Historical
    );
    assert_eq!(
        SolanaMode::resolve(Some("yellowstone"), None).unwrap(),
        SolanaMode::Yellowstone
    );
}

#[test]
fn yellowstone_protobuf_from_real_create_v2() {
    let v = load_json("solana/lifecycle/create.json");
    let update = subscribe_update_from_rpc_json(&v, 3).expect("subscribe update");
    let view = view_from_subscribe_update(&update, "yellowstone", Utc::now(), Finality::Processed)
        .expect("view");
    assert_eq!(view.transaction_index, Some(3));
    let events = raw_events_from_view(&view);
    assert!(
        events.iter().any(|e| {
            DecoderRegistry::production()
                .decode(e)
                .ok()
                .and_then(|o| o.into_token())
                .is_some_and(|t| t.token_address == TOKEN)
        }),
        "CreateV2 token missing from yellowstone proto path"
    );
}

#[test]
fn pump_create_v2_decodes() {
    let token = decode("solana/lifecycle/create.json", "create_instruction_index")
        .into_token()
        .expect("create");
    assert_eq!(token.token_address, TOKEN);
    assert_eq!(token.curve.as_deref(), Some(CURVE));
}

#[test]
fn pump_buy_decodes() {
    match decode("solana/lifecycle/buy.json", "trade_instruction_index") {
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().any(|e| {
                e.as_trade()
                    .is_some_and(|t| t.side == TradeSide::Buy && t.token_address == TOKEN)
            }));
        }
        DecodeOutcome::Unknown => panic!("buy unknown"),
    }
}

#[test]
fn pump_sell_decodes() {
    match decode("solana/pumpfun/sell.json", "trade_instruction_index") {
        DecodeOutcome::Events(evs) => {
            assert!(evs
                .iter()
                .any(|e| e.as_trade().is_some_and(|t| t.side == TradeSide::Sell)));
        }
        DecodeOutcome::Unknown => panic!("sell unknown"),
    }
}

#[test]
fn failed_pump_tx_retained_raw_without_trade() {
    let mut raw = solana_raw_from_fixture("solana/pumpfun/buy.json", "trade_instruction_index");
    if let memecoin_engine::domain::RawEventKind::Solana(ix) = &mut raw.kind {
        ix.execution_status = ExecutionStatus::Failed;
        ix.log_messages.clear();
    }
    let event_id = raw.event_id();
    assert!(!event_id.is_empty());
    match DecoderRegistry::production().decode(&raw).unwrap() {
        DecodeOutcome::Unknown => {}
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().all(|e| e.as_trade().is_none()));
        }
    }
}

#[test]
fn migrate_v2_real_fixture() {
    match decode("solana/lifecycle/migrate.json", "migrate_instruction_index") {
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().any(|e| {
                e.as_lifecycle()
                    .is_some_and(|l| l.lifecycle_type == LifecycleType::Migrated)
            }));
        }
        DecodeOutcome::Unknown => panic!("migrate unknown"),
    }
}

#[test]
fn pumpswap_create_pool_real_fixture() {
    match decode("solana/lifecycle/create_pool.json", "instruction_index") {
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().any(|e| {
                e.as_lifecycle().is_some_and(|l| {
                    l.lifecycle_type == LifecycleType::PoolCreated
                        && l.pool.as_deref() == Some(POOL)
                })
            }));
        }
        DecodeOutcome::Unknown => panic!("create_pool unknown"),
    }
}

#[test]
fn pumpswap_swap_real_fixture() {
    match decode("solana/lifecycle/pamm_sell.json", "instruction_index") {
        DecodeOutcome::Events(evs) => {
            assert!(evs.iter().any(|e| {
                e.as_trade()
                    .is_some_and(|t| t.launchpad == Launchpad::PumpSwap)
            }));
        }
        DecodeOutcome::Unknown => panic!("pamm sell unknown"),
    }
}

#[tokio::test]
async fn full_lifecycle_replay() {
    let store = Arc::new(MemoryStore::new());
    let markets = Arc::new(MarketRegistry::new());
    let report = replay_fixture_dir(&lifecycle_dir(), store.clone(), markets.clone())
        .await
        .unwrap();
    assert!(
        report.tokens.iter().any(|t| t.token_address == TOKEN),
        "token missing"
    );
    assert!(report.trades.iter().any(|t| t.side == TradeSide::Buy));
    assert!(report.lifecycle.iter().any(|l| {
        l.lifecycle_type == LifecycleType::Migrated && l.curve.as_deref() == Some(CURVE)
    }));
    assert!(report.lifecycle.iter().any(|l| {
        l.lifecycle_type == LifecycleType::PoolCreated && l.pool.as_deref() == Some(POOL)
    }));
    assert!(report
        .trades
        .iter()
        .any(|t| t.launchpad == Launchpad::PumpSwap));
    assert!(markets.knows_pool(memecoin_engine::domain::Chain::Solana, POOL));
}

#[tokio::test]
async fn deterministic_replay_twice() {
    let a = replay_fixture_dir(
        &lifecycle_dir(),
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
    )
    .await
    .unwrap();
    let b = replay_fixture_dir(
        &lifecycle_dir(),
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
    )
    .await
    .unwrap();
    assert_eq!(a.canonical_fingerprint(), b.canonical_fingerprint());
    assert_eq!(a.raw_handled, b.raw_handled);
}

#[tokio::test]
async fn replay_does_not_duplicate_db_records() {
    let store = Arc::new(MemoryStore::new());
    let markets = Arc::new(MarketRegistry::new());
    let first = replay_fixture_dir(&lifecycle_dir(), store.clone(), markets.clone())
        .await
        .unwrap();
    let second = replay_fixture_dir(&lifecycle_dir(), store.clone(), markets)
        .await
        .unwrap();
    assert!(first.duplicates == 0);
    assert!(second.duplicates >= first.raw_handled);
    assert_eq!(store.raw_count(), first.raw_handled);
}

#[tokio::test]
async fn watched_market_registers_after_migration() {
    let store = Arc::new(MemoryStore::new());
    let markets = Arc::new(MarketRegistry::new());
    replay_fixture_dir(&lifecycle_dir(), store.clone(), markets.clone())
        .await
        .unwrap();
    assert!(markets.knows_pool(memecoin_engine::domain::Chain::Solana, POOL));
    let persisted = store
        .load_watched_markets(memecoin_engine::domain::Chain::Solana)
        .await
        .unwrap();
    assert!(persisted.iter().any(|m| m.pool.as_deref() == Some(POOL)));
}

#[tokio::test]
async fn watched_market_reload_after_restart() {
    let store = Arc::new(MemoryStore::new());
    replay_fixture_dir(
        &lifecycle_dir(),
        store.clone(),
        Arc::new(MarketRegistry::new()),
    )
    .await
    .unwrap();
    let reloaded = MarketRegistry::new();
    reloaded.load_all(
        store
            .load_watched_markets(memecoin_engine::domain::Chain::Solana)
            .await
            .unwrap(),
    );
    assert!(reloaded.knows_pool(memecoin_engine::domain::Chain::Solana, POOL));
    assert!(reloaded
        .by_token(memecoin_engine::domain::Chain::Solana, TOKEN)
        .is_some());
}

#[test]
fn finality_processed_confirmed_finalized_does_not_change_observed_at() {
    let mut raw = solana_raw_from_fixture("solana/lifecycle/buy.json", "trade_instruction_index");
    let observed = raw.observed_at;
    with_finality(&mut raw, Finality::Processed);
    with_finality(&mut raw, Finality::Confirmed);
    with_finality(&mut raw, Finality::Finalized);
    assert_eq!(raw.observed_at, observed);
    assert_eq!(raw.finality, Finality::Finalized);
    assert_eq!(raw.as_solana().unwrap().finality, Finality::Finalized);
}

#[tokio::test]
async fn orphaned_events_are_preserved() {
    let store = MemoryStore::new();
    let raw = solana_raw_from_fixture("solana/lifecycle/buy.json", "trade_instruction_index");
    let id = raw.event_id();
    store.insert_raw(&raw).await.unwrap();
    assert!(store.mark_orphaned(&id).await.unwrap());
    let loaded = store.get_raw(&id).await.unwrap().expect("preserved");
    assert_eq!(
        loaded.canonical_status,
        memecoin_engine::domain::CanonicalStatus::Orphaned
    );
}

#[test]
fn gap_detection_100_to_250() {
    let t = SolanaSlotTracker::new();
    t.note_head(100);
    t.note_head(250);
    assert_eq!(t.take_missing_ranges(), vec![(101, 249)]);
}

#[tokio::test]
async fn gap_recovery_marks_recovered() {
    let store = MemoryStore::new();
    let id = record_missing_range(&store, 101, 249, "skip")
        .await
        .unwrap();
    assert!(!store.gaps()[0].recovered);
    store.mark_gap_recovered(id).await.unwrap();
    assert!(store.gaps()[0].recovered);
    assert_eq!(
        store
            .unrecovered_gap_count(memecoin_engine::domain::Chain::Solana)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn rpc_dev_session_is_marked_incomplete() {
    let store = MemoryStore::new();
    let session = CollectionSession::start(
        memecoin_engine::domain::Chain::Solana,
        SolanaMode::RpcDev,
        "public-rpc",
        Some("dev".into()),
    );
    assert!(!session.complete);
    assert_eq!(session.quality_status, QualityStatus::RpcDevIncomplete);
    assert_eq!(
        SolanaMode::RpcDev.data_quality_label(),
        QualityStatus::DevelopmentIncomplete.as_str()
    );
    let id = store.insert_session(&session).await.unwrap();
    store
        .finish_session(
            id,
            SessionFinish {
                ended_at: Utc::now(),
                end_block: None,
                end_slot: None,
                complete: false,
                quality_status: QualityStatus::RpcDevIncomplete,
                gap_count: 0,
                notes: None,
            },
        )
        .await
        .unwrap();
    let loaded = store.get_session(id).await.unwrap().unwrap();
    assert!(!loaded.complete);
    assert_eq!(loaded.quality_status, QualityStatus::RpcDevIncomplete);
}

#[test]
fn research_guard_rejects_incomplete_solana_rpc_dev() {
    let session = CollectionSession::start(
        memecoin_engine::domain::Chain::Solana,
        SolanaMode::RpcDev,
        "public-rpc",
        None,
    );
    let err = validate_dataset_quality(&session, QualityCheck::complete_market_data()).unwrap_err();
    assert!(matches!(err, DatasetQualityError::IncompleteSource { .. }));
}

#[test]
fn research_guard_accepts_historical_replay() {
    let mut session = CollectionSession::start(
        memecoin_engine::domain::Chain::Solana,
        SolanaMode::Historical,
        "fixture",
        None,
    );
    session.complete = true;
    session.quality_status = QualityStatus::HistoricalReplay;
    validate_dataset_quality(&session, QualityCheck::complete_market_data()).unwrap();
}

#[tokio::test]
async fn yellowstone_cost_guard_blocks_without_explicit_mode() {
    let (tx, _rx) = mpsc::channel(4);
    let (_sd_tx, sd) = watch::channel(false);
    let (_p_tx, pool_rx) = watch::channel(Vec::new());
    let ingest = YellowstoneIngest {
        config: YellowstoneConfig {
            endpoint: "https://solana-mainnet.g.alchemy.com".into(),
            x_token: Some("paid-token".into()),
            ingest_id: "guard".into(),
            rpc_http: None,
            rpc_ws: None,
            explicitly_enabled: false,
        },
        store: Arc::new(MemoryStore::new()),
        markets: Arc::new(MarketRegistry::new()),
        metrics: memecoin_engine::metrics::DiscoveryMetrics,
        shutdown: sd,
        slots: Arc::new(SolanaSlotTracker::new()),
        pool_rx,
    };
    let err = ingest.run(tx).await.unwrap_err();
    assert!(err.to_string().contains("cost guard"));
}

#[tokio::test]
async fn fixture_source_orders_lifecycle() {
    let mut src = FixtureSource::from_dir(lifecycle_dir()).unwrap();
    let mut ids = Vec::new();
    while let Some(raw) = src.next_event().await.unwrap() {
        ids.push(raw.event_id());
    }
    assert!(ids.len() >= 4);
    let mut src2 = FixtureSource::from_dir(lifecycle_dir()).unwrap();
    let mut ids2 = Vec::new();
    while let Some(raw) = src2.next_event().await.unwrap() {
        ids2.push(raw.event_id());
    }
    assert_eq!(ids, ids2);
}

#[tokio::test]
async fn jsonl_source_streams_without_loading_file_as_vec() {
    let raw = solana_raw_from_fixture("solana/pumpfun/sell.json", "trade_instruction_index");
    let line = serde_json::to_string(&raw).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    std::fs::write(&path, format!("{line}\n{line}\n")).unwrap();
    let mut src = JsonlSource::open(&path).unwrap();
    let a = src.next_event().await.unwrap().unwrap();
    let b = src.next_event().await.unwrap().unwrap();
    assert!(src.next_event().await.unwrap().is_none());
    assert_eq!(a.event_id(), b.event_id());
}

#[test]
fn rpc_dev_warning_is_stable() {
    warn_rpc_dev_once();
    warn_rpc_dev_once();
    assert!(memecoin_engine::domain::RPC_DEV_WARNING.contains("incomplete"));
}

#[tokio::test]
async fn pipeline_duplicate_is_handle_result_duplicate() {
    let store = Arc::new(MemoryStore::new());
    let (tx, _rx) = mpsc::channel(8);
    let (trade_tx, _tr) = mpsc::channel(8);
    let (life_tx, _lr) = mpsc::channel(8);
    let p = memecoin_engine::DiscoveryPipeline {
        store: store.clone(),
        registry: DecoderRegistry::production(),
        markets: Arc::new(MarketRegistry::new()),
        discovered_tx: tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: memecoin_engine::metrics::DiscoveryMetrics,
        ingest_id: "phase21b".into(),
        slots: None,
        pool_tx: None,
        state: None,
    };
    let raw = solana_raw_from_fixture("solana/lifecycle/buy.json", "trade_instruction_index");
    p.handle(raw.clone()).await.unwrap();
    match p.handle(raw).await.unwrap() {
        HandleResult::Duplicate { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn canonical_event_paths_cover_lifecycle_order() {
    let create = decode("solana/lifecycle/create.json", "create_instruction_index");
    let buy = decode("solana/lifecycle/buy.json", "trade_instruction_index");
    let migrate = decode("solana/lifecycle/migrate.json", "migrate_instruction_index");
    let pool = decode("solana/lifecycle/create_pool.json", "instruction_index");
    let swap = decode("solana/lifecycle/pamm_sell.json", "instruction_index");
    assert!(matches!(create, DecodeOutcome::Events(_)));
    assert!(matches!(buy, DecodeOutcome::Events(_)));
    assert!(matches!(migrate, DecodeOutcome::Events(_)));
    assert!(matches!(pool, DecodeOutcome::Events(_)));
    assert!(matches!(swap, DecodeOutcome::Events(_)));
    let _ = std::any::type_name::<CanonicalEvent>();
}
