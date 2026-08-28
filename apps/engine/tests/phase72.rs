use chrono::{TimeZone, Utc};
use memecoin_engine::domain::{
    Chain, DescriptiveLabelQuality, Launchpad, QualityStatus, ResearchCapabilitySet,
};
use memecoin_engine::prospective::{
    clanker_paper_research_valid, in_pons_snipe_window, mark_session_ended, paper_entry,
    shadow_clanker_order, tokens_with_open_positions,
};
use memecoin_engine::sim::descriptive::{is_heartbeat_row, DescriptiveTokenOutcome};
use memecoin_engine::sim::models::SimConfig;
use memecoin_engine::sim::position::SimulatedPosition;
use memecoin_engine::sim::types::{ExecutionStatus, PositionStatus};
use memecoin_engine::sim::ExecutionResult;
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::state::market::{BondingCurveState, MarketState, MarketStateQuality};
use memecoin_engine::state::rolling::RollingWindowSnapshot;
use memecoin_engine::state::snapshot::{SnapshotKind, TokenStateSnapshot, WalletSnapshot};
use memecoin_engine::storage::dbcheck::sanitize_db_error;
use memecoin_engine::wallet::identity_key;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn empty_roll(ms: i64) -> RollingWindowSnapshot {
    RollingWindowSnapshot {
        duration_ms: ms,
        ..Default::default()
    }
}

fn snap(token: &str, pad: Launchpad, t_ms: i64, life: TokenLifecycleState) -> TokenStateSnapshot {
    TokenStateSnapshot {
        id: None,
        chain: if pad == Launchpad::ClankerV4 {
            Chain::Base
        } else {
            Chain::Robinhood
        },
        token_address: token.into(),
        launchpad: pad,
        snapshot_time: ts(t_ms),
        age_ms: t_ms,
        snapshot_kind: SnapshotKind::Periodic,
        lifecycle_trigger: None,
        lifecycle_state: life,
        quote_asset: Some("ETH".into()),
        buy_count_total: 2,
        sell_count_total: 0,
        unique_buyers_total: 2,
        unique_sellers_total: 0,
        buy_quote_volume_raw_total: "100".into(),
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
        curve_progress_bps: Some(1000),
        graduation_progress_bps: None,
        market_state_type: "BONDING_CURVE".into(),
        market_state: MarketState::BondingCurve(BondingCurveState {
            virtual_sol_reserves: Some("30000000000".into()),
            virtual_token_reserves: Some("1073000000000000".into()),
            real_sol_reserves: Some("30000000000".into()),
            real_token_reserves: Some("1073000000000000".into()),
            token_total_supply: None,
            curve_progress_bps: Some(1000),
            last_token_amount_raw: None,
            last_quote_amount_raw: None,
            quality: MarketStateQuality::Complete,
        }),
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
        as_of_event_order: format!("{t_ms}"),
        data_quality: QualityStatus::LiveComplete,
        source_session_id: None,
        canonical_status: memecoin_engine::domain::CanonicalStatus::Canonical,
        finality: memecoin_engine::domain::Finality::Confirmed,
        version: 1,
        superseded: false,
        fingerprint: String::new(),
        created_at: ts(t_ms),
        wallet: WalletSnapshot::default(),
    }
}

fn fill() -> ExecutionResult {
    ExecutionResult::empty(
        memecoin_engine::sim::types::OrderSide::Buy,
        ts(20_000),
        ts(20_000),
        "1000000000000000".into(),
        "1000".into(),
        ExecutionStatus::Filled,
        memecoin_engine::sim::types::ExecutionQuality::Modelled,
        true,
        "",
        1,
        0,
    )
}

#[test]
fn heartbeat_is_not_a_trade() {
    let a = (1_000i64, 1.0f64, 1u64);
    let b = (1_000i64, 1.0f64, 1u64);
    assert!(is_heartbeat_row(Some(&a), &b));
}

#[test]
fn invalid_price_cannot_moonshot() {
    let o = DescriptiveTokenOutcome::from_prices("t", ts(0), None, &[(1_000, 10.0)]);
    assert_eq!(o.quality, DescriptiveLabelQuality::Invalid);
    assert!(!o.reached_10x);
}

#[test]
fn t30_feature_ignores_later_price() {
    let o = DescriptiveTokenOutcome::from_prices(
        "t",
        ts(0),
        Some(1.0),
        &[(20_000, 1.1), (120_000, 50.0)],
    );
    assert!(o.reached_10x);
    assert_eq!(o.time_to_10x_ms, Some(120_000));
    let early: Vec<_> = [(20_000i64, 1.1f64)]
        .into_iter()
        .filter(|(a, _)| *a <= 30_000)
        .collect();
    let early_o = DescriptiveTokenOutcome::from_prices("t", ts(0), Some(1.0), &early);
    assert!(!early_o.reached_10x);
}

#[test]
fn descriptive_not_execution() {
    let o = DescriptiveTokenOutcome::from_prices("t", ts(0), Some(1.0), &[(1_000, 12.0)]);
    assert!(o.reached_10x);
    assert!(!o.capabilities.execution_valid);
    assert!(o.capabilities.descriptive_outcome_valid);
}

#[test]
fn slinky_capabilities_never_upgrade_execution() {
    let s = ResearchCapabilitySet::slinky21_pump_corpus(true);
    assert!(s.feature_valid);
    assert!(s.descriptive_outcome_valid);
    assert!(!s.execution_valid);
    assert!(!s.allows_strategy_pnl());
}

#[test]
fn pons_snipe_window_blocks_paper() {
    let cfg = SimConfig::research_default();
    assert!(in_pons_snipe_window(&cfg, Launchpad::PonsV2, 0));
    assert!(!in_pons_snipe_window(
        &cfg,
        Launchpad::PonsV2,
        cfg.fees.pons_snipe_window_ms + 1
    ));
    let s = snap(
        "tok",
        Launchpad::PonsV2,
        0,
        TokenLifecycleState::CurveActive,
    );
    let r = paper_entry(
        &[s],
        Chain::Robinhood,
        "tok",
        Launchpad::PonsV2,
        ts(0),
        &cfg,
        QualityStatus::LiveComplete,
    );
    assert_eq!(r.status, ExecutionStatus::RejectedQuality);
    assert!(!r.research_valid);
}

#[test]
fn pons_gap_still_unsellable_via_fill_math() {
    let cfg = SimConfig::research_default();
    let s = snap(
        "tok",
        Launchpad::PonsV2,
        30_000,
        TokenLifecycleState::GraduationGap,
    );
    let r = paper_entry(
        std::slice::from_ref(&s),
        Chain::Robinhood,
        "tok",
        Launchpad::PonsV2,
        ts(30_000),
        &cfg,
        QualityStatus::LiveComplete,
    );
    assert!(!matches!(
        r.status,
        ExecutionStatus::Filled | ExecutionStatus::PartialFill
    ));
}

#[test]
fn clanker_shadow_not_research_valid() {
    assert!(!clanker_paper_research_valid());
    let o = shadow_clanker_order(Chain::Base, "tok", ts(1), "1");
    assert_eq!(o.policy_id, "SHADOW_ORDER");
    assert!(!o.result.research_valid);
}

#[test]
fn restart_does_not_duplicate_entry() {
    let mut pos = SimulatedPosition::open(
        1,
        Chain::Robinhood,
        "tok".into(),
        Launchpad::PonsV2,
        "E1".into(),
        &fill(),
        None,
        None,
    );
    pos.simulation_run_id = Some(9);
    let open = tokens_with_open_positions(std::slice::from_ref(&pos));
    assert!(open.contains(&(Chain::Robinhood, "tok".into())));
    let mut positions = vec![pos];
    mark_session_ended(&mut positions, ts(60_000));
    assert_eq!(positions[0].status, PositionStatus::SessionEndedOpen);
    let again = tokens_with_open_positions(&positions);
    assert!(again.contains(&(Chain::Robinhood, "tok".into())));
}

#[test]
fn evm_identity_is_address_only() {
    let a = identity_key("0xABC");
    let b = identity_key("0xabc");
    assert_eq!(a, b);
}

#[tokio::test]
async fn postgres_wallet_identity_and_migration() {
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
    let store = memecoin_engine::storage::postgres::PostgresStore::from_pool(pool.clone());
    store.migrate().await.expect("migrate 0011");
    let t = ts(1_000);
    let id1 = store
        .upsert_evm_wallet(
            "0xAbcDef0000000000000000000000000000000001",
            Chain::Base,
            t,
            true,
            "tokA",
        )
        .await
        .unwrap();
    let id2 = store
        .upsert_evm_wallet(
            "0xabcdef0000000000000000000000000000000001",
            Chain::Robinhood,
            t,
            false,
            "tokB",
        )
        .await
        .unwrap();
    assert_eq!(id1, id2);
    let n = store.count_cross_chain_wallets().await.unwrap();
    assert_eq!(n, 1);
    let check = memecoin_engine::storage::dbcheck::check_database(&url).await;
    assert!(check.ok, "{}", check.message);
}

#[test]
fn db_error_is_sanitized() {
    std::env::set_var(
        "DATABASE_URL",
        "postgres://memecoin:secretpw@127.0.0.1:5435/memecoin",
    );
    let s = sanitize_db_error("password authentication failed for user memecoin:secretpw");
    assert!(s.contains("BLOCKED_DATABASE"));
    assert!(!s.contains("secretpw"));
}
