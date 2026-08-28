use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::{
    CandidateEngine, CandidateInput, CandidatePolicy, CandidateState,
};
use memecoin_engine::domain::{
    CanonicalEvent, CanonicalStatus, Chain, Finality, LaunchMechanism, Launchpad, QualityStatus,
    TokenDiscovered, TradeObserved, TradeSide,
};
use memecoin_engine::features::opt::{count_ratio_bps, OptAmt, OptU64};
use memecoin_engine::features::{
    process_snapshots, write_jsonl, FeatureEngine, FeatureInput, FeatureVector, FEATURE_VERSION,
};
use memecoin_engine::security::assessment::{SecurityAssessment, SecurityVerdict};
use memecoin_engine::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::{StateEngine, TokenStateSnapshot};
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::EventStore;
use std::time::Instant;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn snap_at_or_before<'a>(
    rows: &'a [TokenStateSnapshot],
    token: &str,
    time: chrono::DateTime<Utc>,
) -> Option<&'a TokenStateSnapshot> {
    rows.iter()
        .filter(|s| s.token_address == token && s.snapshot_time <= time)
        .max_by_key(|s| s.snapshot_time)
}

fn discovered_at(token: &str, ms: i64) -> TokenDiscovered {
    TokenDiscovered {
        chain: Chain::Solana,
        chain_id: None,
        token_address: token.into(),
        creator: "creator1".into(),
        launchpad: Launchpad::PumpFun,
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

fn assess(token: &str, verdict: SecurityVerdict, at: chrono::DateTime<Utc>) -> SecurityAssessment {
    let mut ev = Vec::new();
    match verdict {
        SecurityVerdict::Reject => {
            let mut e = SecurityEvidence::new(
                "FREEZE_AUTHORITY",
                EvidenceStatus::Fail,
                Severity::Critical,
                "test",
                "freeze present",
            );
            e.hard_reject = true;
            ev.push(e);
        }
        SecurityVerdict::Unknown => ev.push(SecurityEvidence::new(
            "MINT_ACCOUNT",
            EvidenceStatus::Unknown,
            Severity::Medium,
            "test",
            "missing",
        )),
        SecurityVerdict::Warn => ev.push(SecurityEvidence::new(
            "METADATA",
            EvidenceStatus::Warn,
            Severity::Low,
            "test",
            "mutable",
        )),
        SecurityVerdict::Pass => {}
    }
    let mut a = SecurityAssessment::from_evidence(
        Chain::Solana,
        token,
        Launchpad::PumpFun,
        ev,
        QualityStatus::HistoricalReplay,
        at,
    );
    a.verdict = verdict;
    a
}

fn cand_input<'a>(
    token: &'a str,
    age_ms: i64,
    at: chrono::DateTime<Utc>,
    sec: Option<&'a SecurityAssessment>,
    feats: Option<&'a FeatureVector>,
    trades: u64,
    buyers: u64,
) -> CandidateInput<'a> {
    CandidateInput {
        chain: Chain::Solana,
        token,
        launchpad: Launchpad::PumpFun,
        age_ms,
        as_of_time: at,
        snapshot_id: None,
        security: sec,
        features: feats,
        buy_count: trades,
        unique_buyers: buyers,
        trade_count: trades,
        lifecycle: TokenLifecycleState::CurveActive,
        time_since_last_trade_ms: None,
    }
}

#[test]
fn missing_is_not_zero() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "dead", 1_000,
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime { unix_ms: 31_000 });
    let v = FeatureEngine::compute(FeatureInput::from_history(&snaps[0], &[], None));
    assert_eq!(v.shared.buy_count_total, 0);
    assert!(v.shared.holder_count.is_unknown());
    assert!(v.shared.top10_pct_bps.is_unknown());
    assert!(v.shared.creator_prior_rugs.is_unknown());
    assert!(v.shared.creator_prior_launches.is_unknown());
    assert!(v.shared.bundle_supply_pct_bps.is_unknown());
    assert!(v.shared.estimated_exit_capacity.is_unknown());
    assert!(v.shared.max_notional_at_1pct.is_unknown());
    assert_ne!(v.shared.holder_count, OptU64::value(0));
    assert_ne!(v.shared.creator_prior_rugs, OptU64::value(0));
    assert_eq!(v.feature_version, FEATURE_VERSION);
}

#[test]
fn ratio_divide_by_zero_is_none() {
    assert_eq!(count_ratio_bps(10, 0), None);
    assert_eq!(count_ratio_bps(3, 2), Some(15_000));
}

#[test]
fn unique_buyer_acceleration_positive_and_negative() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "acc", t0,
    ))));
    for i in 0u64..2 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "acc",
            &format!("early{i}"),
            TradeSide::Buy,
            "10",
            "1",
            t0 + 5_000 + i as i64,
            i,
        ))));
    }
    let mid = eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 15_000,
    });
    let s15 = mid
        .iter()
        .filter(|s| s.age_ms >= 15_000)
        .max_by_key(|s| s.snapshot_time)
        .cloned()
        .expect("15s snapshot");
    assert_eq!(s15.rolling_15s.unique_buyers, 2);

    for i in 0u64..8 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "acc",
            &format!("late{i}"),
            TradeSide::Buy,
            "10",
            "1",
            t0 + 20_000 + i as i64 * 100,
            10 + i,
        ))));
    }
    let later = eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 30_000,
    });
    let s30 = later
        .iter()
        .chain(std::iter::once(&s15))
        .filter(|s| s.age_ms >= 30_000)
        .max_by_key(|s| s.snapshot_time)
        .cloned()
        .or_else(|| {
            eng.history
                .iter()
                .filter(|s| s.token_address == "acc" && s.age_ms >= 30_000)
                .max_by_key(|s| s.snapshot_time)
                .cloned()
        })
        .expect("30s snapshot");
    let v = FeatureEngine::compute(FeatureInput::from_history(
        &s30,
        std::slice::from_ref(&s15),
        None,
    ));
    assert_eq!(
        v.shared.unique_buyer_acceleration_15s.as_value(),
        Some(s30.rolling_15s.unique_buyers as i64 - s15.rolling_15s.unique_buyers as i64)
    );
    assert_eq!(
        v.shared.unique_buyer_acceleration_15s.as_value(),
        Some(8 - 2)
    );

    let mut eng2 = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng2.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "neg", t0,
    ))));
    for i in 0u64..8 {
        eng2.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "neg",
            &format!("b{i}"),
            TradeSide::Buy,
            "10",
            "1",
            t0 + 2_000 + i as i64,
            i,
        ))));
    }
    let p = eng2.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 15_000,
    });
    let prior = p
        .iter()
        .filter(|s| s.age_ms >= 15_000)
        .max_by_key(|s| s.snapshot_time)
        .cloned()
        .unwrap();
    let later = eng2.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 30_000,
    });
    let cur = later
        .iter()
        .filter(|s| s.age_ms >= 30_000)
        .max_by_key(|s| s.snapshot_time)
        .cloned()
        .unwrap();
    let vn = FeatureEngine::compute(FeatureInput::from_history(
        &cur,
        std::slice::from_ref(&prior),
        None,
    ));
    assert_eq!(
        vn.shared.unique_buyer_acceleration_15s.as_value(),
        Some(cur.rolling_15s.unique_buyers as i64 - prior.rolling_15s.unique_buyers as i64)
    );
    assert_eq!(vn.shared.unique_buyer_acceleration_15s.as_value(), Some(-8));
}

#[test]
fn no_lookahead_future_trade_does_not_change_earlier_vector() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "nl", t0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "nl",
        "w1",
        TradeSide::Buy,
        "100",
        "10",
        t0 + 10_000,
        1,
    ))));
    eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 30_000,
    });
    let at30 = ts(t0 + 30_000);
    let snap30 = snap_at_or_before(&eng.history, "nl", at30)
        .cloned()
        .unwrap();
    let v1 = FeatureEngine::compute(FeatureInput::from_history(&snap30, &eng.history, None));
    let fp1 = v1.fingerprint.clone();
    let buys1 = v1.shared.buy_count_total;

    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "nl",
        "w2",
        TradeSide::Buy,
        "999",
        "10",
        t0 + 40_000,
        2,
    ))));
    let snap30b = snap_at_or_before(&eng.history, "nl", at30)
        .cloned()
        .unwrap();
    let v2 = FeatureEngine::compute(FeatureInput::from_history(&snap30b, &eng.history, None));
    assert_eq!(fp1, v2.fingerprint);
    assert_eq!(buys1, v2.shared.buy_count_total);
    assert!(eng.get(Chain::Solana, "nl").unwrap().buy_count_total >= 2);
}

#[test]
fn rolling_features_at_30s_60s_2m() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "roll", t0,
    ))));
    for i in 0u64..5 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "roll",
            &format!("w{i}"),
            TradeSide::Buy,
            "50",
            "5",
            t0 + 8_000 + i as i64 * 200,
            i,
        ))));
    }
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "roll",
        "w0",
        TradeSide::Sell,
        "20",
        "2",
        t0 + 12_000,
        99,
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 120_000,
    });
    let pick = |age: i64| {
        snaps
            .iter()
            .chain(eng.history.iter())
            .filter(|s| s.age_ms >= age && s.token_address == "roll")
            .min_by_key(|s| (s.age_ms - age).abs())
            .cloned()
            .unwrap()
    };
    let s30 = pick(30_000);
    let s60 = pick(60_000);
    let s120 = pick(120_000);
    let v30 = FeatureEngine::compute(FeatureInput::from_history(&s30, &eng.history, None));
    let v60 = FeatureEngine::compute(FeatureInput::from_history(&s60, &eng.history, None));
    let v120 = FeatureEngine::compute(FeatureInput::from_history(&s120, &eng.history, None));
    assert_eq!(v30.shared.buy_count_total, 5);
    assert_eq!(v30.shared.sell_count_total, 1);
    assert_eq!(v30.shared.trade_count_imbalance, 4);
    assert!(v30.shared.buy_sell_count_ratio_bps.is_some());
    assert_eq!(v30.shared.win30.unique_buyers, 5);
    assert_eq!(v60.shared.buy_count_total, 5);
    assert_eq!(v120.token_age_ms, s120.age_ms);
    assert_eq!(v30.shared.net_quote_flow_total, "230");
}

#[test]
fn creator_flow_and_protocol_features() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "cr", t0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "cr",
        "creator1",
        TradeSide::Buy,
        "80",
        "8",
        t0 + 2_000,
        1,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "cr",
        "creator1",
        TradeSide::Sell,
        "30",
        "3",
        t0 + 3_000,
        2,
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 15_000,
    });
    let snap = snaps.last().unwrap();
    let v = FeatureEngine::compute(FeatureInput::from_history(snap, &[], None));
    assert!(v.shared.creator_has_sold);
    assert_eq!(v.shared.creator_buy_count, 1);
    assert_eq!(v.shared.creator_sell_count, 1);
    assert_eq!(v.shared.creator_buy_quote_total, "80");
    assert_eq!(v.shared.creator_sell_quote_total, "30");
    assert_eq!(v.shared.creator_net_quote_flow, "50");
    match v.protocol {
        memecoin_engine::features::ProtocolFeatures::SolanaPump { .. } => {}
        other => panic!("expected pump protocol features, got {other:?}"),
    }
}

#[test]
fn liquidity_partial_unknown_not_zero() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    let snaps = eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "liq", 1_000,
    ))));
    let snap = snaps.first().or(eng.history.first()).expect("snapshot");
    let v = FeatureEngine::compute(FeatureInput::from_history(snap, &[], None));
    match &v.shared.liquidity_quote {
        OptAmt::Unknown | OptAmt::Partial { .. } | OptAmt::Value { .. } => {}
    }
    assert!(v.shared.max_notional_at_5pct.is_unknown());
}

#[test]
fn dead_tokens_still_get_feature_vectors() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "dead2", 1_000,
    ))));
    let snaps = eng.finish_all_milestones();
    assert!(snaps.iter().any(|s| s.buy_count_total == 0));
    let batch = process_snapshots(&eng.history, &[], &CandidateEngine::default_research());
    assert!(!batch.vectors.is_empty());
    assert!(batch
        .vectors
        .iter()
        .all(|v| v.shared.trade_count_total == 0));
}

#[test]
fn security_reject_never_eligible_but_still_observed() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "rej", t0,
    ))));
    for i in 0u64..5 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "rej",
            &format!("w{i}"),
            TradeSide::Buy,
            "10",
            "1",
            t0 + 5_000 + i as i64,
            i,
        ))));
    }
    eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 30_000,
    });
    let a = assess("rej", SecurityVerdict::Reject, ts(t0));
    let batch = process_snapshots(&eng.history, &[a], &CandidateEngine::default_research());
    assert!(!batch.vectors.is_empty());
    assert!(batch
        .transitions
        .iter()
        .any(|t| t.to_state == CandidateState::SecurityRejected));
    assert!(!batch
        .transitions
        .iter()
        .any(|t| t.to_state == CandidateState::Eligible));
}

#[test]
fn security_unknown_not_eligible() {
    let eng = CandidateEngine::default_research();
    let a = assess("u", SecurityVerdict::Unknown, ts(1_000));
    let input = cand_input("u", 60_000, ts(61_000), Some(&a), None, 50, 20);
    let steps = eng.step_until_stable(CandidateState::Discovered, &input);
    assert!(steps
        .iter()
        .any(|t| t.to_state == CandidateState::DataIncomplete));
    assert!(!steps.iter().any(|t| t.to_state == CandidateState::Eligible));
}

#[test]
fn watch_confirm_eligible_path() {
    let eng = CandidateEngine::default_research();
    let a = assess("ok", SecurityVerdict::Pass, ts(1_000));
    let t = ts(20_000);
    let steps = eng.step_until_stable(
        CandidateState::Discovered,
        &cand_input("ok", 20_000, t, Some(&a), None, 5, 3),
    );
    let states: Vec<_> = steps.iter().map(|s| s.to_state).collect();
    assert!(states.contains(&CandidateState::Watching));
    assert!(states.contains(&CandidateState::Confirming));
    assert!(states.contains(&CandidateState::Eligible));
    assert_eq!(*states.last().unwrap(), CandidateState::Eligible);
    assert!(!matches!(
        CandidateState::Eligible,
        CandidateState::Discovered
    ));
    assert!(CandidateState::Eligible.is_tradeable_gate());
}

#[test]
fn eligible_is_not_buy() {
    assert_ne!(CandidateState::Eligible.as_str(), "BUY");
    assert!(CandidateState::parse("BUY").is_none());
}

#[test]
fn expire_no_activity() {
    let eng = CandidateEngine::default_research();
    let a = assess("z", SecurityVerdict::Pass, ts(1_000));
    let steps = eng.step_until_stable(
        CandidateState::Discovered,
        &cand_input("z", 300_000, ts(301_000), Some(&a), None, 0, 0),
    );
    assert!(steps
        .iter()
        .any(|t| t.to_state == CandidateState::Expired && t.reason == "NO_ACTIVITY"));
}

#[tokio::test]
async fn candidate_transitions_are_append_only() {
    let store = MemoryStore::new();
    let eng = CandidateEngine::default_research();
    let a = assess("ap", SecurityVerdict::Pass, ts(1_000));
    let s1 = eng
        .step(
            CandidateState::Discovered,
            &cand_input("ap", 6_000, ts(7_000), Some(&a), None, 1, 1),
        )
        .unwrap();
    store.insert_candidate_transition(&s1).await.unwrap();
    let s2 = eng
        .step(
            s1.to_state,
            &cand_input("ap", 20_000, ts(21_000), Some(&a), None, 5, 3),
        )
        .unwrap();
    store.insert_candidate_transition(&s2).await.unwrap();
    let list = store
        .list_candidate_transitions(Chain::Solana, "ap", "default")
        .await
        .unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].from_state, CandidateState::Discovered);
}

#[tokio::test]
async fn parallel_policies_do_not_overwrite() {
    let store = MemoryStore::new();
    let a = assess("pp", SecurityVerdict::Pass, ts(1_000));
    let d = CandidateEngine::new(CandidatePolicy::research_default());
    let c = CandidateEngine::new(CandidatePolicy::conservative());
    let input = cand_input("pp", 20_000, ts(21_000), Some(&a), None, 5, 3);
    for t in d.step_until_stable(CandidateState::Discovered, &input) {
        store.insert_candidate_transition(&t).await.unwrap();
    }
    for t in c.step_until_stable(CandidateState::Discovered, &input) {
        store.insert_candidate_transition(&t).await.unwrap();
    }
    let def = store
        .list_candidate_transitions(Chain::Solana, "pp", "default")
        .await
        .unwrap();
    let cons = store
        .list_candidate_transitions(Chain::Solana, "pp", "conservative")
        .await
        .unwrap();
    assert!(!def.is_empty());
    assert!(!cons.is_empty());
    assert_eq!(def.last().unwrap().to_state, CandidateState::Eligible);
    assert_ne!(
        cons.last().unwrap().to_state,
        CandidateState::Eligible,
        "conservative needs 8 trades / 5 buyers"
    );
}

#[test]
fn deterministic_replay_same_fingerprints() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "det", t0,
    ))));
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "det",
        "w1",
        TradeSide::Buy,
        "10",
        "1",
        t0 + 4_000,
        1,
    ))));
    eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 30_000,
    });
    let a = assess("det", SecurityVerdict::Pass, ts(t0));
    let b1 = process_snapshots(
        &eng.history,
        std::slice::from_ref(&a),
        &CandidateEngine::default_research(),
    );
    let b2 = process_snapshots(
        &eng.history,
        std::slice::from_ref(&a),
        &CandidateEngine::default_research(),
    );
    let fp1: Vec<_> = b1.vectors.iter().map(|v| v.fingerprint.clone()).collect();
    let fp2: Vec<_> = b2.vectors.iter().map(|v| v.fingerprint.clone()).collect();
    assert_eq!(fp1, fp2);
}

#[tokio::test]
async fn jsonl_export_roundtrip() {
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    let snaps = eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "ex", 1_000,
    ))));
    let snaps = if snaps.is_empty() {
        eng.history.clone()
    } else {
        snaps
    };
    let batch = process_snapshots(&snaps, &[], &CandidateEngine::default_research());
    let mut buf = Vec::new();
    let n = write_jsonl(&batch.vectors, &mut buf).unwrap();
    assert_eq!(n, batch.vectors.len());
    let line = std::str::from_utf8(&buf).unwrap().lines().next().unwrap();
    let v: FeatureVector = serde_json::from_str(line).unwrap();
    assert_eq!(v.feature_version, FEATURE_VERSION);
    assert!(line.contains("holder_count"));
    assert!(!line.contains("OPPORTUNITY_SCORE"));
    assert!(!line.contains("opportunity_score"));
    let store = MemoryStore::new();
    store.insert_feature_vector(&v).await.unwrap();
    let listed = store
        .list_feature_vectors(Chain::Solana, "ex")
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
}

#[test]
fn two_thousand_tokens_feature_pass() {
    let started = Instant::now();
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    for i in 0..2_000u32 {
        let tok = format!("t{i}");
        let t0 = 1_000 + i as i64;
        eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
            &tok, t0,
        ))));
        if i % 2 == 0 {
            eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
                &tok,
                "w1",
                TradeSide::Buy,
                "10",
                "1",
                t0 + 1_000,
                1,
            ))));
        }
    }
    let snaps: Vec<_> = eng.history.clone();
    let batch = process_snapshots(&snaps, &[], &CandidateEngine::default_research());
    assert!(batch.vectors.len() >= 2_000);
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 30,
        "2k token feature pass too slow: {elapsed:?}"
    );
}

#[test]
fn repeat_buyers_from_wallet_snapshot() {
    let t0 = 1_000i64;
    let mut eng = StateEngine::replay(QualityStatus::HistoricalReplay, None);
    eng.apply(CanonicalEvent::TokenDiscovered(Box::new(discovered_at(
        "rb", t0,
    ))));
    for i in 0u64..4 {
        eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
            "rb",
            "w1",
            TradeSide::Buy,
            "10",
            "1",
            t0 + 2_000 + i as i64,
            i,
        ))));
    }
    eng.apply(CanonicalEvent::Trade(Box::new(trade_at(
        "rb",
        "w2",
        TradeSide::Buy,
        "10",
        "1",
        t0 + 3_000,
        9,
    ))));
    let snaps = eng.finish_until(memecoin_engine::state::clock::StateTime {
        unix_ms: t0 + 15_000,
    });
    let v = FeatureEngine::compute(FeatureInput::from_history(snaps.last().unwrap(), &[], None));
    assert_eq!(v.shared.repeat_buyer_count.as_value(), Some(1));
    assert_eq!(v.shared.unique_buyers_total, 2);
    assert_eq!(v.shared.buy_count_total, 5);
}

#[tokio::test]
async fn replay_fixtures_with_features() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let markets = std::sync::Arc::new(memecoin_engine::watch::MarketRegistry::new());
    let report = memecoin_engine::replay::replay_fixture_dir_full(
        &memecoin_engine::test_support::fixture_path("solana/lifecycle"),
        store,
        markets,
        true,
        true,
    )
    .await
    .unwrap();
    assert!(!report.snapshots.is_empty());
    assert!(!report.feature_vectors.is_empty());
}
