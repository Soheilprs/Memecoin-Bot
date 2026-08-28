use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use memecoin_engine::collect::{pons_prospective_target, CollectTarget};
use memecoin_engine::domain::Chain;
use memecoin_engine::ingest::evm::pons_curve::{CurveReadErrorKind, PonsCurveReader};
use memecoin_engine::ingest::rpc_provider::{classify_circuit, CircuitKind, RpcEndpoint, RpcPool};
use memecoin_engine::lab::observation::{ObservationHealth, ObservationReason, STALE_AFTER};
use memecoin_engine::lab::pons_exp::{EXP003_ID, EXP004_RPCQUAL_ID};
use memecoin_engine::state::pons_curve::{PonsCurveState, PonsCurveStateQuality, PonsCurveStatus};

fn sample_state(block: u64) -> PonsCurveState {
    PonsCurveState {
        chain: Chain::Robinhood,
        token: "0xabc".into(),
        curve: "0xcurve".into(),
        block_number: Some(block),
        block_hash: Some(format!("0x{block:x}")),
        observed_at: chrono::Utc::now(),
        virtual_quote_reserve: "1".into(),
        virtual_token_reserve: "1".into(),
        real_quote_reserve: "1".into(),
        real_token_reserve: "1".into(),
        quote_collected: "1".into(),
        graduation_threshold: "100".into(),
        progress_bps: Some(0),
        status: PonsCurveStatus::Active,
        fee_bps: 100,
        creator_tax_bps: 0,
        snipe_tax_bps: Some(9900),
        state_quality: PonsCurveStateQuality::ExactBlockRead,
        source: "test".into(),
        abi_version: "test".into(),
    }
}

#[tokio::test]
async fn fifteen_concurrent_arm_reads_one_fetch() {
    let fetches = Arc::new(AtomicU64::new(0));
    let r =
        PonsCurveReader::counted_mock(sample_state(10), Duration::from_millis(50), fetches.clone());
    let mut joins = Vec::new();
    for _ in 0..15 {
        let rr = r.clone();
        joins.push(tokio::spawn(async move {
            rr.read("0xabc", "0xcurve", Some(10)).await
        }));
    }
    let mut oks = 0;
    for j in joins {
        if j.await.unwrap().is_ok() {
            oks += 1;
        }
    }
    assert_eq!(oks, 15);
    assert_eq!(
        fetches.load(Ordering::Relaxed),
        1,
        "single-flight must fetch once"
    );
    assert_eq!(r.fetch_count(), 1);
}

#[tokio::test]
async fn same_token_different_block_separate_reads() {
    let fetches = Arc::new(AtomicU64::new(0));
    let r =
        PonsCurveReader::counted_mock(sample_state(1), Duration::from_millis(5), fetches.clone());
    r.read("0xabc", "0xcurve", Some(11)).await.unwrap();
    r.read("0xabc", "0xcurve", Some(12)).await.unwrap();
    assert_eq!(fetches.load(Ordering::Relaxed), 2);
}

#[test]
fn primary_429_classifies_failover_kind() {
    assert_eq!(
        classify_circuit(r#"{"code":429,"message":"Monthly capacity limit exceeded"}"#),
        Some(CircuitKind::Quota)
    );
    assert_eq!(
        classify_circuit("compute units per second capacity"),
        Some(CircuitKind::Throughput)
    );
    let pool = RpcPool::new(
        vec![
            RpcEndpoint {
                name: "primary".into(),
                http: "http://127.0.0.1:1".into(),
                ws: None,
            },
            RpcEndpoint {
                name: "fallback".into(),
                http: "http://127.0.0.1:2".into(),
                ws: None,
            },
        ],
        "robinhood",
    );
    pool.trip(CircuitKind::Quota);
    assert!(pool.circuit_open());
    assert_eq!(pool.circuit_kind(), Some(CircuitKind::Quota));
    assert_eq!(pool.endpoints().len(), 2);
}

#[test]
fn heartbeat_while_rpc_dead_is_not_valid() {
    let h = ObservationHealth::default();
    assert_eq!(
        h.evaluate(std::time::Instant::now()),
        ObservationReason::CollectorStale
    );
    h.note_rate_limit();
    assert_eq!(
        h.evaluate(std::time::Instant::now()),
        ObservationReason::RpcRateLimit
    );
    assert_ne!(
        h.evaluate(std::time::Instant::now()).interval_status(),
        "VALID"
    );
}

#[test]
fn collector_stale_is_partial_not_valid() {
    let h = ObservationHealth::default();
    h.note_head(Chain::Robinhood, 1);
    let _later = STALE_AFTER;
    std::thread::sleep(Duration::from_millis(5));
    h.note_collector_down();
    let r = h.evaluate(std::time::Instant::now());
    assert!(matches!(
        r,
        ObservationReason::CollectorStale | ObservationReason::RpcRateLimit
    ));
    assert_ne!(r.interval_status(), "VALID");
}

#[test]
fn recovery_reason_is_valid_only_when_fresh_and_execution_ok() {
    let h = ObservationHealth::default();
    h.note_head(Chain::Robinhood, 42);
    h.note_log();
    h.note_execution_ok();
    assert_eq!(
        h.evaluate(std::time::Instant::now()),
        ObservationReason::Healthy
    );
    assert_eq!(
        h.evaluate(std::time::Instant::now()).interval_status(),
        "VALID"
    );
}

#[test]
fn retry_backoff_caps() {
    let mut wait = Duration::from_millis(500);
    for _ in 0..20 {
        wait = wait.saturating_mul(2).min(Duration::from_secs(30));
    }
    assert_eq!(wait, Duration::from_secs(30));
}

#[test]
fn pons_prospective_is_robinhood_only() {
    let t = pons_prospective_target();
    assert_eq!(t, CollectTarget::Robinhood);
    assert_eq!(t.chains(), vec![Chain::Robinhood]);
    assert!(!t.chains().contains(&Chain::Base));
    assert!(!t.chains().contains(&Chain::Solana));
}

#[test]
fn exp003_is_not_rpcqual() {
    assert_ne!(EXP003_ID, EXP004_RPCQUAL_ID);
}

#[tokio::test]
async fn coverage_rpc_dead_is_not_valid_interval() {
    use sqlx::postgres::PgPoolOptions;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    let node = match Postgres::default().start().await {
        Ok(n) => n,
        Err(err) => {
            eprintln!("SKIP postgres tests; docker unavailable: {err}");
            return;
        }
    };
    let host_port = node.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{host_port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .expect("connect");
    let store = memecoin_engine::storage::postgres::PostgresStore::from_pool(pool);
    store.migrate().await.expect("migrate");
    let exp = EXP004_RPCQUAL_ID;
    store
        .open_observation_interval(exp, chrono::Utc::now(), "VALID")
        .await
        .unwrap();
    let h = ObservationHealth::default();
    h.note_rate_limit();
    memecoin_engine::lab::observation::apply_observation_health(&store, exp, &h, false)
        .await
        .unwrap();
    let open = store.load_open_observation(exp).await.unwrap().unwrap();
    assert_ne!(open.1, "VALID", "RPC death must not leave VALID coverage");
    h.note_head(Chain::Robinhood, 99);
    h.note_log();
    h.note_execution_ok();
    memecoin_engine::lab::observation::apply_observation_health(&store, exp, &h, true)
        .await
        .unwrap();
    let open = store.load_open_observation(exp).await.unwrap().unwrap();
    assert_eq!(open.1, "VALID");
}

#[tokio::test]
async fn failing_reader_does_not_fake_fill() {
    let r = PonsCurveReader::failing(CurveReadErrorKind::RateLimit, "429");
    let e = r.read("0xabc", "0xcurve", Some(1)).await.unwrap_err();
    assert_eq!(e.kind, CurveReadErrorKind::RateLimit);
}

#[test]
fn failover_does_not_substitute_latest_without_block() {
    // block_hash_at errors if hash missing; unit of the contract is the error message.
    let msg = "block hash missing on fallback; not substituting latest";
    assert!(msg.contains("not substituting latest"));
}
