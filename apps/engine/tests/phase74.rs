//! Phase 7.4: Pons curve-state reads, paper fills, failure taxonomy, P0–P4 lock.

use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::CandidateState;
use memecoin_engine::domain::{CanonicalStatus, Chain, Finality, Launchpad, QualityStatus};
use memecoin_engine::ingest::evm::pons_curve::{
    classify_paper_failure, classify_rpc, execution_quality_label, CurveReadErrorKind,
    PonsCurveReader,
};
use memecoin_engine::sim::descriptive::OutcomeMaturity;
use memecoin_engine::sim::impact::executable_fill;
use memecoin_engine::sim::models::SimConfig;
use memecoin_engine::sim::position::SimulatedPosition;
use memecoin_engine::sim::types::{ExecutionQuality, ExecutionStatus, OrderSide, PositionStatus};
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::market::MarketState;
use memecoin_engine::state::pons_curve::{
    decode_abi_bool, decode_abi_words, overlay_snapshot, PonsCurveState, PonsCurveStateQuality,
    PonsCurveStatus, PONS_CURVE_ABI_VERSION,
};
use memecoin_engine::state::rolling::RollingWindowSnapshot;
use memecoin_engine::state::snapshot::{SnapshotKind, TokenStateSnapshot, WalletSnapshot};
use memecoin_engine::strategy::{smoke_decide, ProspectivePolicy, StrategyContext};

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

fn blank_snap(token: &str, age_ms: i64, life: TokenLifecycleState) -> TokenStateSnapshot {
    TokenStateSnapshot {
        id: None,
        chain: Chain::Robinhood,
        token_address: token.into(),
        launchpad: Launchpad::PonsV2,
        snapshot_time: ts(age_ms),
        age_ms,
        snapshot_kind: SnapshotKind::Milestone,
        lifecycle_trigger: None,
        lifecycle_state: life,
        quote_asset: Some("QUOTE".into()),
        buy_count_total: 3,
        sell_count_total: 0,
        unique_buyers_total: 3,
        unique_sellers_total: 0,
        buy_quote_volume_raw_total: "0".into(),
        sell_quote_volume_raw_total: "0".into(),
        buy_token_volume_raw_total: "0".into(),
        sell_token_volume_raw_total: "0".into(),
        creator_buy_count: 0,
        creator_sell_count: 0,
        creator_buy_quote_raw: "0".into(),
        creator_sell_quote_raw: "0".into(),
        last_trade_side: None,
        last_trade_token_raw: None,
        last_trade_quote_raw: None,
        last_trade_token_decimals: Some(18),
        last_trade_quote_decimals: Some(18),
        curve_progress_bps: None,
        graduation_progress_bps: None,
        market_state_type: "UNKNOWN".into(),
        market_state: MarketState::Unknown,
        rolling_5s: empty_roll(5_000),
        rolling_15s: empty_roll(15_000),
        rolling_30s: empty_roll(30_000),
        rolling_60s: empty_roll(60_000),
        rolling_120s: empty_roll(120_000),
        rolling_300s: empty_roll(300_000),
        rolling_900s: empty_roll(900_000),
        as_of_event_id: None,
        as_of_block: None,
        as_of_slot: None,
        as_of_event_order: format!("{age_ms}"),
        data_quality: QualityStatus::LiveComplete,
        source_session_id: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        version: 1,
        superseded: false,
        fingerprint: String::new(),
        created_at: ts(age_ms),
        wallet: WalletSnapshot::default(),
    }
}

fn sample_state(quality: PonsCurveStateQuality) -> PonsCurveState {
    PonsCurveState {
        chain: Chain::Robinhood,
        token: "0xabc".into(),
        curve: "0xcurve".into(),
        block_number: Some(47_000_000),
        block_hash: Some("0xdead".into()),
        observed_at: ts(30_000),
        virtual_quote_reserve: "30000000000000000000".into(),
        virtual_token_reserve: "1073000000000000000000000000".into(),
        real_quote_reserve: "1000000000000000000".into(),
        real_token_reserve: "800000000000000000000000000".into(),
        quote_collected: "1000000000000000000".into(),
        graduation_threshold: "10000000000000000000".into(),
        progress_bps: Some(1000),
        status: PonsCurveStatus::Active,
        fee_bps: 100,
        creator_tax_bps: 100,
        snipe_tax_bps: Some(9900),
        state_quality: quality,
        source: "test".into(),
        abi_version: PONS_CURVE_ABI_VERSION.into(),
    }
}

#[test]
fn decode_get_reserves_two_words() {
    let hex = format!("0x{:0>64x}{:0>64x}", 1_000u64, 2_000u64);
    let words = decode_abi_words(&hex);
    assert_eq!(words, vec!["1000".to_string(), "2000".to_string()]);
    assert!(!decode_abi_bool("0x0"));
    assert!(decode_abi_bool(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
    ));
}

#[test]
fn progress_from_reserves_integer() {
    let bps = PonsCurveState::progress_from_reserves("25", "100");
    assert_eq!(bps, Some(2_500));
    assert_eq!(PonsCurveState::progress_from_reserves("1", "0"), None);
}

#[test]
fn overlay_makes_executable_fill() {
    let mut snap = blank_snap("0xabc", 30_000, TokenLifecycleState::CurveActive);
    let st = sample_state(PonsCurveStateQuality::ExactBlockRead);
    overlay_snapshot(&mut snap, &st);
    assert!(matches!(snap.market_state, MarketState::BondingCurve(_)));
    assert_eq!(snap.as_of_block, Some(47_000_000));
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let mut fees = fees;
    fees.pons_curve_bps = st.quote_fee_bps();
    assert_eq!(fees.pons_curve_bps, 200);
    let slip = memecoin_engine::sim::models::SlippageModel::none();
    let fill = executable_fill(
        &snap,
        OrderSide::Buy,
        "1000000000",
        &fees,
        &slip,
        10_000,
        false,
    );
    assert!(
        fill.status.is_fill(),
        "expected fill, got {:?} {:?}",
        fill.status,
        fill.reason
    );
    assert!(fill.protocol_fee.parse::<u128>().unwrap() > 0 || fees.pons_curve_bps == 0);
}

#[test]
fn unknown_reserves_without_overlay() {
    let snap = blank_snap("0xabc", 30_000, TokenLifecycleState::CurveActive);
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let slip = memecoin_engine::sim::models::SlippageModel::none();
    let fill = executable_fill(
        &snap,
        OrderSide::Buy,
        "1000000000",
        &fees,
        &slip,
        10_000,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::UnavailableMarketState);
    assert_eq!(
        fill.reason.as_deref(),
        Some("UNKNOWN_LIQUIDITY_NOT_INFINITE")
    );
}

#[test]
fn graduation_gap_not_faked() {
    let mut snap = blank_snap("0xabc", 30_000, TokenLifecycleState::GraduationGap);
    overlay_snapshot(
        &mut snap,
        &sample_state(PonsCurveStateQuality::ExactBlockRead),
    );
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let slip = memecoin_engine::sim::models::SlippageModel::none();
    let fill = executable_fill(
        &snap,
        OrderSide::Buy,
        "1000000000",
        &fees,
        &slip,
        10_000,
        false,
    );
    assert_eq!(fill.status, ExecutionStatus::TemporarilyUnavailable);
    assert_eq!(
        classify_paper_failure(fill.reason.as_deref(), fill.status),
        "GRADUATION_GAP"
    );
}

#[test]
fn snipe_window_blocks_smoke_entry() {
    let cfg = SimConfig::research_default();
    let mut snap = blank_snap("0xabc", 500, TokenLifecycleState::CurveActive);
    overlay_snapshot(
        &mut snap,
        &sample_state(PonsCurveStateQuality::LiveLatestRead),
    );
    let r = memecoin_engine::prospective::paper_entry_at(
        &[snap],
        Chain::Robinhood,
        "0xabc",
        Launchpad::PonsV2,
        ts(500),
        ts(500),
        &cfg,
        QualityStatus::LiveComplete,
    );
    assert_eq!(r.status, ExecutionStatus::RejectedQuality);
    assert_eq!(r.reason.as_deref(), Some("PONS_SNIPE_WINDOW"));
}

#[test]
fn failure_taxonomy_covers_spec_reasons() {
    use ExecutionStatus::*;
    assert_eq!(
        classify_paper_failure(Some("PROVIDER_TIMEOUT: x"), UnavailableMarketState),
        "PROVIDER_TIMEOUT"
    );
    assert_eq!(
        classify_paper_failure(Some("PROVIDER_RATE_LIMIT: 429"), UnavailableMarketState),
        "PROVIDER_RATE_LIMIT"
    );
    assert_eq!(
        classify_paper_failure(Some("CURVE_NOT_FOUND"), UnavailableMarketState),
        "CURVE_NOT_FOUND"
    );
    assert_eq!(
        classify_paper_failure(Some("INVALID_CURVE_STATE"), UnavailableMarketState),
        "INVALID_CURVE_STATE"
    );
    assert_eq!(
        classify_paper_failure(Some("PONS_GRADUATION_GAP"), TemporarilyUnavailable),
        "GRADUATION_GAP"
    );
    assert_eq!(
        classify_paper_failure(Some("IMPACT_9000_GT_MAX_100"), RejectedSlippage),
        "SLIPPAGE_LIMIT"
    );
    assert_eq!(
        classify_paper_failure(Some("SEEDED_FAILURE"), ExecutionStatus::Failed),
        "EXECUTION_FAILURE_MODEL"
    );
    assert_eq!(
        classify_paper_failure(Some("ZERO_TOKEN_OUT"), RejectedLiquidity),
        "INSUFFICIENT_LIQUIDITY"
    );
    assert_eq!(
        classify_rpc("timeout waiting for response").kind,
        CurveReadErrorKind::Timeout
    );
    assert_eq!(
        classify_rpc("429 too many requests").kind,
        CurveReadErrorKind::RateLimit
    );
}

#[test]
fn exact_block_read_is_research_valid_live_paper() {
    assert!(PonsCurveStateQuality::ExactBlockRead.research_valid_live_paper());
    assert!(PonsCurveStateQuality::LiveLatestRead.research_valid_live_paper());
    assert!(!PonsCurveStateQuality::Partial.research_valid_live_paper());
    assert!(!PonsCurveStateQuality::Unknown.research_valid_live_paper());
    assert_eq!(
        execution_quality_label(PonsCurveStateQuality::ExactBlockRead, true),
        "MODELLED_HIGH_CONFIDENCE"
    );
}

#[tokio::test]
async fn mock_reader_returns_pinned_block() {
    let st = sample_state(PonsCurveStateQuality::ExactBlockRead);
    let reader = PonsCurveReader::mock(st.clone());
    let got = reader
        .read("0xabc", "0xcurve", Some(47_000_000))
        .await
        .unwrap();
    assert_eq!(got.block_number, Some(47_000_000));
    assert_eq!(got.virtual_quote_reserve, st.virtual_quote_reserve);
    assert_eq!(got.abi_version, PONS_CURVE_ABI_VERSION);
}

#[tokio::test]
async fn timeout_is_not_zero_reserve() {
    let reader = PonsCurveReader::failing(CurveReadErrorKind::Timeout, "deadline");
    let err = reader.read("0xabc", "0xcurve", Some(1)).await.unwrap_err();
    assert_eq!(err.kind, CurveReadErrorKind::Timeout);
    assert_eq!(err.kind.as_str(), "PROVIDER_TIMEOUT");
}

#[test]
fn outcome_maturity_pending_until_one_hour() {
    assert_eq!(
        OutcomeMaturity::for_live_age(30_000),
        OutcomeMaturity::Pending
    );
    assert_eq!(
        OutcomeMaturity::for_live_age(3_600_000),
        OutcomeMaturity::Mature
    );
    assert_eq!(
        OutcomeMaturity::CensoredSessionEnd.as_str(),
        "CENSORED_SESSION_END"
    );
}

#[test]
fn filled_position_survives_json_roundtrip() {
    let mut snap = blank_snap("0xabc", 30_000, TokenLifecycleState::CurveActive);
    overlay_snapshot(
        &mut snap,
        &sample_state(PonsCurveStateQuality::ExactBlockRead),
    );
    let fees = memecoin_engine::sim::models::FeeModel::research_default();
    let slip = memecoin_engine::sim::models::SlippageModel::none();
    let fill = executable_fill(
        &snap,
        OrderSide::Buy,
        "1000000000",
        &fees,
        &slip,
        10_000,
        false,
    );
    assert!(fill.status.is_fill());
    let mut fill = memecoin_engine::sim::exec::ExecutionResult::empty(
        OrderSide::Buy,
        ts(30_000),
        ts(31_000),
        "1000000000".into(),
        "0".into(),
        fill.status,
        ExecutionQuality::Modelled,
        false,
        "",
        1,
        0,
    );
    fill.filled_token = "12345".into();
    fill.filled_quote = "1000000000".into();
    fill.curve_state_quality = Some("EXACT_BLOCK_READ".into());
    fill.execution_quality_label = Some("MODELLED_HIGH_CONFIDENCE".into());
    let pos = SimulatedPosition::open(
        7,
        Chain::Robinhood,
        "0xabc".into(),
        Launchpad::PonsV2,
        "PIPELINE_SMOKE_POLICY".into(),
        &fill,
        Some(1),
        Some(2),
    );
    let v = serde_json::to_value(&pos).unwrap();
    let back: SimulatedPosition = serde_json::from_value(v).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.remaining_token_amount, "12345");
    assert_eq!(back.quote_cost, "1000000000");
    assert_eq!(back.strategy_policy_id, "PIPELINE_SMOKE_POLICY");
    assert_eq!(back.entry_feature_vector_id, Some(1));
    assert_eq!(back.status, PositionStatus::Open);
    assert!(!back.entry_research_valid);
}

#[test]
fn p0_p4_definitions_unchanged() {
    let ids: Vec<_> = ProspectivePolicy::all()
        .into_iter()
        .map(|p| p.id())
        .collect();
    assert_eq!(
        ids,
        vec![
            "P0_FIRST_ELIGIBLE_CONTROL",
            "P1_SOLANA_BUYERS_3_30S",
            "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
            "P3_PRICE_WITHOUT_BUYERS_AVOID",
            "P4_LOW_PARTICIPATION_FILTER",
        ]
    );
    let cfg = SimConfig::research_default();
    let _ = smoke_decide(
        &StrategyContext {
            features: None,
            candidate: CandidateState::Discovered,
            security: Some(memecoin_engine::security::assessment::SecurityVerdict::Pass),
            first_eligible_at: None,
            now: ts(20_000),
            token: "x",
            seed: 1,
        },
        &cfg,
    );
}

#[test]
fn abi_version_pinned() {
    assert_eq!(PONS_CURVE_ABI_VERSION, "v2-bondingcurve-getters-1");
    let a = memecoin_engine::artifacts::pons_curve_views_artifact();
    assert_eq!(a.version, PONS_CURVE_ABI_VERSION);
    assert!(a.source.contains("PonsV2BondingCurve.sol"));
}
