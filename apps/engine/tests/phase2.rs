use std::sync::Arc;

use memecoin_engine::decoders::{DecodeOutcome, DecoderRegistry};
use memecoin_engine::domain::{
    CanonicalStatus, Chain, Launchpad, LifecycleType, RawEventKind, TradeSide,
};
use memecoin_engine::pipeline::{DiscoveryPipeline, HandleResult};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::{Checkpoint, EventStore};
use memecoin_engine::test_support::{
    evm_raw_from_fixture, pumpfun_raw_from_fixture, solana_raw_from_fixture,
};
use memecoin_engine::watch::MarketRegistry;
use tokio::sync::mpsc;

fn pipeline(store: Arc<MemoryStore>) -> DiscoveryPipeline<MemoryStore> {
    let (tx, _rx) = mpsc::channel(32);
    let (trade_tx, _tr) = mpsc::channel(32);
    let (life_tx, _lr) = mpsc::channel(32);
    DiscoveryPipeline {
        store,
        registry: DecoderRegistry::production(),
        markets: Arc::new(MarketRegistry::new()),
        discovered_tx: tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: memecoin_engine::metrics::DiscoveryMetrics,
        ingest_id: "phase2".into(),
        slots: None,
        pool_tx: None,
        state: None,
    }
}

fn first_trade(raw: &memecoin_engine::domain::RawEvent) -> memecoin_engine::domain::TradeObserved {
    match DecoderRegistry::production().decode(raw).unwrap() {
        DecodeOutcome::Events(events) => events
            .into_iter()
            .find_map(|e| match e {
                memecoin_engine::domain::CanonicalEvent::Trade(t) => Some(*t),
                _ => None,
            })
            .expect("trade"),
        DecodeOutcome::Unknown => panic!("unknown"),
    }
}

fn first_life(
    raw: &memecoin_engine::domain::RawEvent,
) -> memecoin_engine::domain::LifecycleObserved {
    match DecoderRegistry::production().decode(raw).unwrap() {
        DecodeOutcome::Events(events) => events
            .into_iter()
            .find_map(|e| match e {
                memecoin_engine::domain::CanonicalEvent::Lifecycle(t) => Some(*t),
                _ => None,
            })
            .expect("lifecycle"),
        DecodeOutcome::Unknown => panic!("unknown"),
    }
}

#[tokio::test]
async fn pumpfun_buy_fixture_decodes() {
    let raw = solana_raw_from_fixture("solana/pumpfun/buy.json", "trade_instruction_index");
    let trade = first_trade(&raw);
    assert_eq!(trade.chain, Chain::Solana);
    assert_eq!(trade.launchpad, Launchpad::PumpFun);
    assert_eq!(trade.side, TradeSide::Buy);
    assert!(!trade.base_amount_raw.is_empty());
    assert!(!trade.quote_amount_raw.is_empty());
    assert!(!trade.base_amount_raw.contains('.'));
    assert_eq!(trade.token_address.chars().last(), Some('p'));
}

#[tokio::test]
async fn pumpfun_sell_fixture_decodes() {
    let raw = solana_raw_from_fixture("solana/pumpfun/sell.json", "trade_instruction_index");
    let trade = first_trade(&raw);
    assert_eq!(trade.side, TradeSide::Sell);
    assert!(!trade.base_amount_raw.contains('e'));
    assert_eq!(trade.quote_decimals, 9);
}

#[tokio::test]
async fn pons_curve_buy_fixture_decodes() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    let trade = first_trade(&raw);
    assert_eq!(trade.chain, Chain::Robinhood);
    assert_eq!(trade.side, TradeSide::Buy);
    assert_eq!(trade.base_decimals, 18);
    assert!(trade.curve.is_some());
    assert!(!trade.base_amount_raw.contains('.'));
}

#[tokio::test]
async fn pons_curve_sell_fixture_decodes() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/curve_sell.json");
    let trade = first_trade(&raw);
    assert_eq!(trade.side, TradeSide::Sell);
    assert!(trade.quote_amount_raw.chars().all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn pons_launch_swept_and_pool_graduated_are_distinct() {
    let swept = first_life(&evm_raw_from_fixture("robinhood/pons_v2/launch_swept.json"));
    let grad = first_life(&evm_raw_from_fixture(
        "robinhood/pons_v2/pool_graduated.json",
    ));
    assert_eq!(swept.lifecycle_type, LifecycleType::LaunchSwept);
    assert_eq!(grad.lifecycle_type, LifecycleType::PoolGraduated);
    assert_ne!(swept.tx_hash_or_signature, grad.tx_hash_or_signature);
    assert!(swept.block_number.unwrap() <= grad.block_number.unwrap());
}

#[tokio::test]
async fn clanker_v4_swap_fixture_decodes() {
    let created = evm_raw_from_fixture("base/clanker_v4/token_created_for_swap.json");
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    match p.handle(created).await.unwrap() {
        HandleResult::Discovered(_) => {}
        other => panic!("{other:?}"),
    }
    let raw = evm_raw_from_fixture("base/clanker_v4/swap.json");
    let result = p.handle(raw).await.unwrap();
    match result {
        HandleResult::Canonical { trades, .. } => assert_eq!(trades, 1),
        HandleResult::Discovered(_) => {}
        other => panic!("{other:?}"),
    }
    let trades = store.trades();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].chain, Chain::Base);
    assert!(!trades[0].base_amount_raw.contains('.'));
}

#[tokio::test]
async fn same_block_trades_keep_log_index_order() {
    let buy = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    let mut sell = evm_raw_from_fixture("robinhood/pons_v2/curve_sell.json");
    if let RawEventKind::Evm(log) = &mut sell.kind {
        if let RawEventKind::Evm(b) = &buy.kind {
            log.block_number = b.block_number;
            log.transaction_index = b.transaction_index;
            log.log_index = b.log_index + 1;
            log.transaction_hash =
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        }
    }
    let a = first_trade(&buy);
    let b = first_trade(&sell);
    assert!(a.order_key() < b.order_key());
}

#[tokio::test]
async fn overlapping_backfill_does_not_duplicate_trades() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    let mut live = raw.clone();
    live.source = "evm_ws".into();
    let mut backfill = raw.clone();
    backfill.source = "evm_backfill".into();
    match p.handle(live).await.unwrap() {
        HandleResult::Canonical { trades: 1, .. } => {}
        other => panic!("{other:?}"),
    }
    match p.handle(backfill).await.unwrap() {
        HandleResult::Duplicate { .. } => {}
        other => panic!("expected duplicate {other:?}"),
    }
    assert_eq!(store.trades().len(), 1);
}

#[tokio::test]
async fn checkpoint_restart_backfill_is_continuous() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    p.handle(raw.clone()).await.unwrap();
    let cp = store
        .load_checkpoint("robinhood:live")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cp.last_block, raw.block_number());
    let plan = memecoin_engine::ingest::ResumePlan::for_evm(
        Some(&cp),
        (cp.last_block.unwrap() as u64) + 30,
    );
    assert!(plan.from_block.unwrap() < cp.last_block.unwrap() as u64);
    match p.handle(raw).await.unwrap() {
        HandleResult::Duplicate { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn trade_reorg_orphans_does_not_delete() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    let id = raw.event_id();
    p.handle(raw.clone()).await.unwrap();
    p.mark_removed(&id, Chain::Robinhood).await.unwrap();
    let stored = store.get_raw(&id).await.unwrap().unwrap();
    assert_eq!(stored.canonical_status, CanonicalStatus::Orphaned);
    assert!(store.get_trade(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn large_integer_amount_round_trips() {
    let mut raw = evm_raw_from_fixture("robinhood/pons_v2/curve_buy.json");
    let huge = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    if let RawEventKind::Evm(log) = &mut raw.kind {
        // four uint256 words of all-ones still decode as uint256 max for quoteIn
        log.data = format!("0x{}", "ff".repeat(32 * 4));
    }
    let trade = first_trade(&raw);
    assert_eq!(trade.quote_amount_raw, huge);
    assert!(!trade.quote_amount_raw.contains('.'));
    assert!(!trade.quote_amount_raw.contains('e'));
}

#[tokio::test]
async fn create_still_emits_token_discovered() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store);
    match p.handle(pumpfun_raw_from_fixture()).await.unwrap() {
        HandleResult::Discovered(t) => assert_eq!(t.launchpad, Launchpad::PumpFun),
        other => panic!("{other:?}"),
    }
}

#[test]
fn resume_plan_uses_overlap() {
    let mut cp = Checkpoint::new("x", Chain::Base);
    cp.last_block = Some(1000);
    cp.overlap_blocks = 64;
    let plan = memecoin_engine::ingest::ResumePlan::for_evm(Some(&cp), 1030);
    assert_eq!(plan.from_block, Some(936));
}
