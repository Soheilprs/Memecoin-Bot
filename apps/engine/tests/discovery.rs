use std::sync::Arc;

use memecoin_engine::decoders::Decoder;
use memecoin_engine::decoders::{DecoderRegistry, PonsV2Decoder, PumpfunDecoder};
use memecoin_engine::domain::raw_event::normalize_address;
use memecoin_engine::domain::{Chain, Launchpad, RawEventKind};
use memecoin_engine::pipeline::{DiscoveryPipeline, HandleResult};
use memecoin_engine::registry::{CLANKER_ABI_VERSION, PONS_ABI_VERSION, PUMPFUN_IDL_VERSION};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::EventStore;
use memecoin_engine::test_support::{
    evm_raw_from_fixture, evm_raw_from_value, load_json, pumpfun_raw_from_fixture, unknown_evm,
};
use pretty_assertions::assert_eq;
use tokio::sync::mpsc;

fn pipeline(store: Arc<MemoryStore>) -> DiscoveryPipeline<MemoryStore> {
    let (tx, _rx) = mpsc::channel(16);
    let (trade_tx, _tr) = mpsc::channel(16);
    let (life_tx, _lr) = mpsc::channel(16);
    DiscoveryPipeline {
        store,
        registry: DecoderRegistry::production(),
        markets: std::sync::Arc::new(memecoin_engine::watch::MarketRegistry::new()),
        discovered_tx: tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: memecoin_engine::metrics::DiscoveryMetrics,
        ingest_id: "test".into(),
        slots: None,
        pool_tx: None,
        state: None,
    }
}

fn expect_token(
    raw: &memecoin_engine::domain::RawEvent,
) -> memecoin_engine::domain::TokenDiscovered {
    DecoderRegistry::production()
        .decode(raw)
        .unwrap()
        .into_token()
        .expect("expected token")
}

#[tokio::test]
async fn pumpfun_fixture_decodes() {
    let raw = pumpfun_raw_from_fixture();
    let expected = load_json("solana/pumpfun/create_v2.json");
    let token = expect_token(&raw);
    assert_eq!(token.chain, Chain::Solana);
    assert_eq!(
        token.token_address,
        expected["expected"]["token_address"].as_str().unwrap()
    );
    assert_eq!(
        token.creator,
        expected["expected"]["creator"].as_str().unwrap()
    );
    assert_eq!(token.launchpad, Launchpad::PumpFun);
    assert_eq!(
        token.curve.as_deref(),
        expected["expected"]["curve"].as_str()
    );
    assert_eq!(token.bonding_curve, true);
    assert_eq!(token.decoder_version, PUMPFUN_IDL_VERSION);
    assert_eq!(token.instruction_index, Some(2));
}

#[tokio::test]
async fn pons_v2_fixture_decodes() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let expected = load_json("robinhood/pons_v2/token_launched.json");
    let token = expect_token(&raw);
    assert_eq!(token.chain, Chain::Robinhood);
    assert_eq!(token.chain_id, Some(4663));
    assert_eq!(
        token.token_address,
        normalize_address(expected["expected"]["token_address"].as_str().unwrap())
    );
    assert_eq!(
        token.creator,
        normalize_address(expected["expected"]["creator"].as_str().unwrap())
    );
    let curve = normalize_address(expected["expected"]["curve"].as_str().unwrap());
    assert_eq!(token.curve.as_deref(), Some(curve.as_str()));
    assert_eq!(token.launchpad, Launchpad::PonsV2);
    assert_eq!(token.bonding_curve, true);
    assert_eq!(token.decoder_version, PONS_ABI_VERSION);
}

#[tokio::test]
async fn clanker_v4_fixture_decodes() {
    let raw = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    let expected = load_json("base/clanker_v4/token_created.json");
    let token = expect_token(&raw);
    assert_eq!(token.chain, Chain::Base);
    assert_eq!(
        token.token_address,
        normalize_address(expected["expected"]["token_address"].as_str().unwrap())
    );
    assert_eq!(
        token.creator,
        normalize_address(expected["expected"]["creator"].as_str().unwrap())
    );
    assert_eq!(token.launchpad, Launchpad::ClankerV4);
    assert_eq!(
        token.quote_asset.as_deref(),
        Some("0x4200000000000000000000000000000000000006")
    );
    assert_eq!(token.pool.as_deref(), expected["expected"]["pool"].as_str());
    assert_eq!(token.bonding_curve, false);
    assert_eq!(token.decoder_version, CLANKER_ABI_VERSION);
}

#[tokio::test]
async fn unknown_robinhood_factory_does_not_crash() {
    let raw = unknown_evm(
        Chain::Robinhood,
        "0x000000000000000000000000000000000000dead",
    );
    let outcome = DecoderRegistry::production().decode(&raw).unwrap();
    assert!(matches!(
        outcome,
        memecoin_engine::decoders::DecodeOutcome::Unknown
    ));
}

#[tokio::test]
async fn unknown_base_log_does_not_crash() {
    let raw = unknown_evm(Chain::Base, "0x000000000000000000000000000000000000beef");
    let outcome = DecoderRegistry::production().decode(&raw).unwrap();
    assert!(matches!(
        outcome,
        memecoin_engine::decoders::DecodeOutcome::Unknown
    ));
}

#[tokio::test]
async fn malformed_event_returns_structured_error() {
    let mut raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    if let RawEventKind::Evm(log) = &mut raw.kind {
        log.data = "0xdead".into();
    }
    let err = DecoderRegistry::production().decode(&raw).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("malformed") || msg.contains("decode"), "{msg}");
}

#[tokio::test]
async fn evm_event_idempotent_100_times() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let mut discovered = 0;
    let mut dup = 0;
    for _ in 0..100 {
        match p.handle(raw.clone()).await.unwrap() {
            HandleResult::Discovered(_) => discovered += 1,
            HandleResult::Duplicate { .. } => dup += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(discovered, 1);
    assert_eq!(dup, 99);
}

#[tokio::test]
async fn solana_event_idempotent_100_times() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = pumpfun_raw_from_fixture();
    let mut discovered = 0;
    let mut dup = 0;
    for _ in 0..100 {
        match p.handle(raw.clone()).await.unwrap() {
            HandleResult::Discovered(_) => discovered += 1,
            HandleResult::Duplicate { .. } => dup += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(discovered, 1);
    assert_eq!(dup, 99);
}

#[tokio::test]
async fn evm_removed_marks_orphaned_not_deleted() {
    let store = Arc::new(MemoryStore::new());
    let p = pipeline(store.clone());
    let raw = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    let id = raw.event_id();
    p.handle(raw.clone()).await.unwrap();
    let mut removed = raw.clone();
    if let RawEventKind::Evm(log) = &mut removed.kind {
        log.removed = true;
    }
    let result = p.handle(removed).await.unwrap();
    assert!(matches!(
        result,
        HandleResult::Duplicate { .. } | HandleResult::Orphaned { .. }
    ));
    p.mark_removed(&id, Chain::Base).await.unwrap();
    let stored = store.get_raw(&id).await.unwrap().expect("raw remains");
    assert!(matches!(
        stored.canonical_status,
        memecoin_engine::domain::raw_event::CanonicalStatus::Orphaned
    ));
    assert!(store.get_discovered(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn replay_is_byte_equivalent() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let a = expect_token(&raw);
    let b = expect_token(&raw);
    let ja = serde_json::to_vec(&a).unwrap();
    let jb = serde_json::to_vec(&b).unwrap();
    assert_eq!(ja, jb);
}

#[test]
fn abi_idl_pinning() {
    let pump = memecoin_engine::artifacts::pumpfun_artifact();
    let pons = memecoin_engine::artifacts::pons_artifact();
    let clanker = memecoin_engine::artifacts::clanker_artifact();
    assert_eq!(pump.version, PUMPFUN_IDL_VERSION);
    assert_eq!(pons.version, PONS_ABI_VERSION);
    assert_eq!(clanker.version, CLANKER_ABI_VERSION);
    assert_eq!(
        pump.sha256,
        memecoin_engine::artifacts::sha256_hex(pump.bytes)
    );
    assert!(!pump.bytes.is_empty());
    assert!(memecoin_engine::artifacts::artifact_for("pons_v2", "not-a-version").is_none());
    assert!(memecoin_engine::artifacts::artifact_for("pons_v2", PONS_ABI_VERSION).is_some());
}

#[test]
fn unknown_decoder_version_does_not_use_newest() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let decoder = PonsV2Decoder::with_version("not-pinned");
    let err = decoder.decode(&raw).unwrap_err();
    match err {
        memecoin_engine::error::EngineError::DecoderVersionMismatch {
            requested, pinned, ..
        } => {
            assert_eq!(requested, "not-pinned");
            assert_eq!(pinned, PONS_ABI_VERSION);
        }
        other => panic!("{other}"),
    }
    let pump = pumpfun_raw_from_fixture();
    let err = PumpfunDecoder::with_version("9.9.9")
        .decode(&pump)
        .unwrap_err();
    assert!(matches!(
        err,
        memecoin_engine::error::EngineError::DecoderVersionMismatch { .. }
    ));
}

#[test]
fn decoder_registry_selects_correct_decoder() {
    let pump = pumpfun_raw_from_fixture();
    let pons = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let clanker = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    let registry = DecoderRegistry::production();
    for (raw, launchpad) in [
        (pump, Launchpad::PumpFun),
        (pons, Launchpad::PonsV2),
        (clanker, Launchpad::ClankerV4),
    ] {
        let t = registry.decode(&raw).unwrap().into_token().expect("token");
        assert_eq!(t.launchpad, launchpad);
    }
}

#[test]
fn timestamps_are_distinct_fields() {
    let raw = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    assert!(raw.chain_time().is_some());
    assert_ne!(raw.observed_at, raw.chain_time().unwrap());
    let token = expect_token(&raw);
    assert!(token.chain_timestamp.is_some());
    assert_eq!(token.observed_at, raw.observed_at);
}

#[tokio::test]
async fn resume_plan_replays_overlap() {
    use memecoin_engine::ingest::ResumePlan;
    use memecoin_engine::storage::Checkpoint;
    let cp = Checkpoint {
        ingest_id: "base-ws".into(),
        chain: Chain::Base,
        stream: "default".into(),
        last_block: Some(1000),
        last_block_hash: None,
        last_finalized_block: None,
        last_slot: None,
        last_confirmed_slot: None,
        last_finalized_slot: None,
        last_signature: None,
        overlap_blocks: 64,
        overlap_slots: 32,
    };
    let plan = ResumePlan::for_evm(Some(&cp), 1100);
    assert_eq!(plan.from_block, Some(1000 - 64));
    let scp = Checkpoint {
        ingest_id: "sol".into(),
        chain: Chain::Solana,
        stream: "default".into(),
        last_block: None,
        last_block_hash: None,
        last_finalized_block: None,
        last_slot: Some(500),
        last_confirmed_slot: None,
        last_finalized_slot: None,
        last_signature: None,
        overlap_blocks: 64,
        overlap_slots: 32,
    };
    let splan = ResumePlan::for_solana(Some(&scp), 600);
    assert_eq!(splan.from_slot, Some(500 - 32));
}

#[tokio::test]
async fn security_interface_is_unknown_not_safe() {
    use memecoin_engine::security::traits::{FastSecurityVerdict, NoopSecurity, SecurityFast};
    let raw = pumpfun_raw_from_fixture();
    let token = expect_token(&raw);
    let result = NoopSecurity.check(&token).await.unwrap();
    assert_eq!(result.verdict, FastSecurityVerdict::Unknown);
}

#[tokio::test]
async fn postgres_migrations_apply_and_are_idempotent() {
    use memecoin_engine::storage::postgres::PostgresStore;
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
    let store = PostgresStore::from_pool(pool.clone());
    store.migrate().await.expect("first migrate");
    store.migrate().await.expect("second migrate");
    let count: i64 = sqlx::query_scalar("select count(*) from factories")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(count >= 3);
    let sessions_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'collection_sessions')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sessions_exist);
    let snaps_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'token_state_snapshots')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(snaps_exist);
    let sec_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'security_assessments')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sec_exist);
    let feat_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'feature_vectors')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(feat_exist);
    let cand_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'candidate_state_transitions')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(cand_exist);
    let cur_cand: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'token_current_candidate')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(cur_cand);
    let sim_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'simulation_runs')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sim_exist);
    let out_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'token_outcomes')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(out_exist);
    let exp_exist: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_name = 'strategy_experiments')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exp_exist);

    let p = {
        let (tx, _rx) = mpsc::channel(8);
        DiscoveryPipeline {
            store: Arc::new(store.clone()),
            registry: DecoderRegistry::production(),
            markets: std::sync::Arc::new(memecoin_engine::watch::MarketRegistry::new()),
            discovered_tx: tx,
            trade_tx: {
                let (t, _) = mpsc::channel(8);
                t
            },
            lifecycle_tx: {
                let (t, _) = mpsc::channel(8);
                t
            },
            metrics: memecoin_engine::metrics::DiscoveryMetrics,
            ingest_id: "pg-test".into(),
            slots: None,
            pool_tx: None,
            state: None,
        }
    };
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    match p.handle(raw.clone()).await.unwrap() {
        HandleResult::Discovered(_) => {}
        other => panic!("{other:?}"),
    }
    match p.handle(raw.clone()).await.unwrap() {
        HandleResult::Duplicate { .. } => {}
        other => panic!("expected duplicate {other:?}"),
    }
    let id = raw.event_id();
    p.mark_removed(&id, Chain::Robinhood).await.unwrap();
    let still: i64 = sqlx::query_scalar("select count(*) from raw_events where id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still, 1);
    let status: String =
        sqlx::query_scalar("select canonical_status from raw_events where id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "orphaned");
}

#[test]
fn evm_raw_from_value_roundtrip() {
    let v = load_json("robinhood/pons_v2/token_launched.json");
    let raw = evm_raw_from_value(&v);
    assert_eq!(raw.chain(), Chain::Robinhood);
}
