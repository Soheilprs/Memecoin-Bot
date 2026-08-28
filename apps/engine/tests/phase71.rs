use chrono::{TimeZone, Utc};
use memecoin_engine::decoders::{DecodeOutcome, DecoderRegistry};
use memecoin_engine::domain::{
    classify_amount, AmountQuality, CanonicalEvent, CanonicalStatus, CorpusEventType, CorpusRecord,
    CorpusSourceKind, DecoderStatus, Finality, IdentityQuality, QualityStatus, RawEvent,
    RawEventKind, TradeSide, IMPORTER_VERSION, SLKY_DATASET_ID,
};
use memecoin_engine::historical::corpus::raw_from_record;
use memecoin_engine::historical::HistoricalSource;
use memecoin_engine::historical::{
    detect_hour_gaps, graduation_bias, scan_raw_events, sha256_bytes, validate_historical_dataset,
    DatasetManifest, DatasetVerdict, GraduationBias, PumpCorpusSource, StreamingScan,
};
use memecoin_engine::lab::exp001::{
    exp001_may_run_test, exp001_verdict, lifecycle_split_ok, refuse_if_not_locked_once,
};
use memecoin_engine::lab::experiment::StrategyExperiment;
use memecoin_engine::lab::split::{assign_split, chronological_split, SplitKind};
use memecoin_engine::replay::{replay_corpus_jsonl, ReplayOpts};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::watch::MarketRegistry;
use std::collections::BTreeSet;
use std::io::Write;
use std::sync::Arc;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn rec(
    event_type: CorpusEventType,
    mint: &str,
    row: u64,
    seq: u64,
    t_ms: i64,
    extra: impl FnOnce(&mut CorpusRecord),
) -> CorpusRecord {
    let mut c = CorpusRecord {
        source_kind: CorpusSourceKind::DecodedResearchCorpus,
        dataset_id: SLKY_DATASET_ID.into(),
        source_file: "tokens.parquet".into(),
        source_row: row,
        event_type,
        identity_quality: IdentityQuality::Derived,
        mint: mint.into(),
        creator: Some("creator1".into()),
        trader: None,
        side: None,
        token_amount: None,
        sol_amount: None,
        amount_quality: AmountQuality::Missing,
        timestamp: ts(t_ms),
        seconds_since_launch_milli: None,
        slot: None,
        signature: None,
        transaction_index: None,
        instruction_index: None,
        inner_instruction_index: None,
        v_sol_bonding_curve: None,
        v_tokens_bonding_curve: None,
        data_quality: "FLOAT_DERIVED".into(),
        normalization_version: "7.1.0".into(),
        order_seq: seq,
        original: serde_json::json!({}),
    };
    extra(&mut c);
    c
}

fn raw(c: CorpusRecord) -> RawEvent {
    raw_from_record(c, "historical:pumpfun_corpus")
}

fn fixture_events() -> Vec<RawEvent> {
    let dead = "DeadMint111111111111111111111111111111111pump";
    let short = "ShortMint11111111111111111111111111111111pump";
    let win = "WinMint1111111111111111111111111111111111pump";
    vec![
        raw(rec(CorpusEventType::Launch, dead, 0, 0, 1_000, |_| {})),
        raw(rec(CorpusEventType::Launch, short, 1, 1, 1_000, |_| {})),
        raw(rec(CorpusEventType::Launch, win, 2, 2, 1_000, |_| {})),
        raw(rec(CorpusEventType::Trade, short, 10, 10, 31_000, |c| {
            c.source_file = "trades.parquet".into();
            c.trader = Some("walletA".into());
            c.side = Some(TradeSide::Buy);
            c.sol_amount = Some("0.01".into());
            c.token_amount = Some("1000.5".into());
            c.amount_quality = AmountQuality::FloatNotInteger;
            c.seconds_since_launch_milli = Some(30_000);
        })),
        raw(rec(CorpusEventType::Trade, win, 11, 11, 61_000, |c| {
            c.source_file = "trades.parquet".into();
            c.trader = Some("walletB".into());
            c.side = Some(TradeSide::Buy);
            c.sol_amount = Some("0.02".into());
            c.amount_quality = AmountQuality::FloatNotInteger;
            c.seconds_since_launch_milli = Some(60_000);
        })),
        raw(rec(
            CorpusEventType::Graduation,
            win,
            20,
            20,
            120_000,
            |c| {
                c.source_file = "migrations.parquet".into();
                c.original = serde_json::json!({"pool_address": "realPool111"});
            },
        )),
    ]
}

#[test]
fn manifest_hash_is_deterministic() {
    let mut a = DatasetManifest::slinky21_template("2026-08-27T00:00:00Z");
    a.original_files = vec![memecoin_engine::historical::FileChecksum {
        path: "tokens.parquet".into(),
        size_bytes: 10,
        sha256: "aa".into(),
    }];
    let mut b = a.clone();
    let ha = a.compute_dataset_hash();
    let hb = b.compute_dataset_hash();
    assert_eq!(ha, hb);
    b.importer_version = "9.9.9".into();
    assert_ne!(ha, b.compute_dataset_hash());
    assert_eq!(sha256_bytes(b"abc").len(), 64);
    assert_eq!(a.importer_version, IMPORTER_VERSION);
}

#[test]
fn classify_amount_does_not_invent_lamports() {
    assert_eq!(
        classify_amount(Some("123")),
        (AmountQuality::OnchainInteger, Some("123".into()))
    );
    assert_eq!(
        classify_amount(Some("30000000000.0")).0,
        AmountQuality::IntegerValuedFloat
    );
    assert_eq!(
        classify_amount(Some("0.01")).0,
        AmountQuality::FloatNotInteger
    );
    assert_eq!(classify_amount(None).0, AmountQuality::Missing);
}

#[tokio::test]
async fn streaming_importer_does_not_require_full_ram_api() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("corpus-{}.jsonl", std::process::id()));
    {
        let mut f = std::fs::File::create(&dir).unwrap();
        for ev in fixture_events() {
            writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).unwrap();
        }
    }
    let mut src = PumpCorpusSource::open(&dir).unwrap();
    let mut n = 0usize;
    while src.next_event().await.unwrap().is_some() {
        n += 1;
    }
    assert_eq!(n, 6);
    let _ = std::fs::remove_file(&dir);
}

#[test]
fn event_order_is_deterministic_not_random() {
    let mut events = fixture_events();
    events.reverse();
    events.sort_by_key(|e| e.as_corpus().unwrap().order_key());
    let types: Vec<_> = events
        .iter()
        .map(|e| e.as_corpus().unwrap().event_type)
        .collect();
    assert_eq!(types[0], CorpusEventType::Launch);
    assert_eq!(*types.last().unwrap(), CorpusEventType::Graduation);
    let k1 = events[0].as_corpus().unwrap().order_key();
    let k2 = events[1].as_corpus().unwrap().order_key();
    assert!(k1 <= k2);
}

#[test]
fn dedup_reports_exact_duplicates() {
    let mut events = fixture_events();
    events.push(events[3].clone());
    let (scan, _c, dups, _) = scan_raw_events(events.iter());
    assert!(dups.exact_duplicate_rows >= 1 || scan.dup_event_ids >= 1);
}

#[test]
fn dead_tokens_are_preserved() {
    let events = fixture_events();
    let (_s, coverage, _, _) = scan_raw_events(events.iter());
    assert_eq!(coverage.launches, 3);
    assert_eq!(coverage.zero_trade, 1);
    assert_eq!(coverage.graduated, 1);
    assert!(coverage.launches > coverage.graduated);
}

#[test]
fn graduation_bias_detector() {
    assert_eq!(graduation_bias(100, 100), GraduationBias::GraduatedOnly);
    assert_eq!(graduation_bias(100, 1), GraduationBias::AllLaunches);
    assert_eq!(graduation_bias(0, 0), GraduationBias::Unknown);
}

#[test]
fn temporal_gap_detector() {
    let mut hours = BTreeSet::new();
    hours.insert(10);
    hours.insert(11);
    hours.insert(20);
    let gaps = detect_hour_gaps(&hours);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].start, "12");
    assert_eq!(gaps[0].end, "19");
}

#[test]
fn dataset_validation_gate_feature_only_not_execution() {
    let events = fixture_events();
    let (scan, coverage, dups, missing) = scan_raw_events(events.iter());
    let v = validate_historical_dataset(None, &scan, &coverage, &dups, &missing);
    assert!(v.schema_valid);
    assert!(v.dead_tokens_present);
    assert!(v.launch_population_valid);
    assert!(v.feature_valid);
    assert!(!v.execution_valid);
    assert!(!v.trade_amounts_valid);
    assert!(!v.curve_reconstructable);
    assert_eq!(v.verdict, DatasetVerdict::FeatureOnly);
    assert_eq!(v.quality_status, QualityStatus::HistoricalPartial);
    assert_eq!(v.identity_quality, IdentityQuality::Derived);
    assert_eq!(exp001_may_run_test(&v), Err("EXP001_BLOCKED_DATASET"));
    assert_eq!(exp001_verdict(&v).as_str(), "EXP001_BLOCKED_DATASET");
}

#[test]
fn survivor_only_corpus_is_invalid_for_exp001() {
    let win = "WinMint1111111111111111111111111111111111pump";
    let events = [
        raw(rec(CorpusEventType::Launch, win, 0, 0, 1_000, |_| {})),
        raw(rec(CorpusEventType::Graduation, win, 1, 1, 2_000, |_| {})),
    ];
    let (scan, coverage, dups, missing) = scan_raw_events(events.iter());
    let v = validate_historical_dataset(None, &scan, &coverage, &dups, &missing);
    assert!(!v.launch_population_valid);
    assert_eq!(v.graduation_bias, GraduationBias::GraduatedOnly);
    assert_eq!(v.verdict, DatasetVerdict::Invalid);
}

#[test]
fn split_isolation_token_lifecycle_stays_in_assigned_split() {
    let b = chronological_split(ts(0), ts(100_000));
    let discovery = assign_split(ts(10_000), &b);
    assert_eq!(discovery, SplitKind::Train);
    assert!(lifecycle_split_ok(discovery, SplitKind::Train));
    assert!(!lifecycle_split_ok(discovery, SplitKind::Test));
}

#[test]
fn config_lock_and_test_rerun_refusal() {
    let mut e = StrategyExperiment::new("EXP001", "exp");
    e.dataset_hash = Some("abc".into());
    e.lock().unwrap();
    e.begin_out_of_sample_test("abc").unwrap();
    assert_eq!(e.test_run_count, 1);
    assert_eq!(e.begin_out_of_sample_test("abc"), Err("TEST_ALREADY_RUN"));
}

#[test]
fn dataset_hash_mismatch_refusal() {
    let mut e = StrategyExperiment::new("EXP001", "exp");
    e.dataset_hash = Some("abc".into());
    e.lock().unwrap();
    assert_eq!(
        e.begin_out_of_sample_test("zzz"),
        Err("DATASET_HASH_MISMATCH")
    );
}

#[test]
fn historical_partial_refuses_oos_test() {
    let mut e = StrategyExperiment::new("EXP001", "exp");
    e.dataset_hash = Some("abc".into());
    e.data_quality = QualityStatus::HistoricalPartial;
    e.lock().unwrap();
    assert_eq!(
        refuse_if_not_locked_once(&mut e, "abc"),
        Err("EXP001_BLOCKED_DATASET")
    );
}

#[test]
fn corpus_decoder_emits_canonical_and_keeps_provenance() {
    let events = fixture_events();
    let reg = DecoderRegistry::production();
    let mut kinds = vec![];
    for ev in &events {
        match reg.decode(ev).unwrap() {
            DecodeOutcome::Events(out) => {
                for e in out {
                    match e {
                        CanonicalEvent::TokenDiscovered(t) => {
                            assert_eq!(t.launchpad.as_str(), "pumpfun");
                            kinds.push("td");
                        }
                        CanonicalEvent::Trade(t) => {
                            assert_eq!(t.quote_amount_raw, "0");
                            assert_eq!(t.metadata["identity_quality"].as_str(), Some("DERIVED"));
                            assert_eq!(t.metadata["integer_fill_usable"].as_bool(), Some(false));
                            kinds.push("tr");
                        }
                        CanonicalEvent::Lifecycle(l) => {
                            kinds.push("lf");
                            assert_eq!(l.lifecycle_type.as_str(), "MIGRATED");
                        }
                    }
                }
            }
            DecodeOutcome::Unknown => panic!("unknown corpus event"),
        }
    }
    assert_eq!(kinds.iter().filter(|k| **k == "td").count(), 3);
    assert_eq!(kinds.iter().filter(|k| **k == "tr").count(), 2);
    assert_eq!(kinds.iter().filter(|k| **k == "lf").count(), 1);
}

#[tokio::test]
async fn corpus_replay_subset_through_existing_pipeline() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("corpus-replay-{}.jsonl", std::process::id()));
    {
        let mut f = std::fs::File::create(&dir).unwrap();
        for ev in fixture_events() {
            writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).unwrap();
        }
    }
    let mut opts = ReplayOpts::corpus(QualityStatus::HistoricalPartial, false);
    opts.snapshots = true;
    opts.features = true;
    let report = replay_corpus_jsonl(
        &dir,
        Arc::new(MemoryStore::new()),
        Arc::new(MarketRegistry::new()),
        opts,
    )
    .await
    .unwrap();
    assert_eq!(report.tokens.len(), 3);
    assert_eq!(report.trades.len(), 2);
    assert_eq!(report.lifecycle.len(), 1);
    assert_eq!(
        report.session.quality_status,
        QualityStatus::HistoricalPartial
    );
    assert!(!report.session.complete);
    let _ = std::fs::remove_file(&dir);
}

#[test]
fn raw_event_roundtrip_decoded_corpus() {
    let ev = fixture_events().remove(0);
    let v = serde_json::to_value(&ev).unwrap();
    let back: RawEvent = serde_json::from_value(v).unwrap();
    assert!(matches!(back.kind, RawEventKind::DecodedCorpus(_)));
    assert_eq!(back.event_id(), ev.event_id());
    assert_eq!(back.canonical_status, CanonicalStatus::Canonical);
    assert_eq!(back.finality, Finality::Finalized);
    assert_eq!(back.decoder_status, DecoderStatus::Pending);
}

#[test]
fn streaming_scan_matches_batch() {
    let events = fixture_events();
    let mut s = StreamingScan::new(true);
    for e in &events {
        s.push(e);
    }
    let (a, c1, d1, m1) = s.finish();
    let (b, c2, d2, m2) = scan_raw_events(events.iter());
    assert_eq!(a, b);
    assert_eq!(c1, c2);
    assert_eq!(d1, d2);
    assert_eq!(m1, m2);
}
