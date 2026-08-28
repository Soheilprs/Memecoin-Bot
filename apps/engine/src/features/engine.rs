use chrono::{DateTime, Timelike, Utc};

use crate::domain::Launchpad;
use crate::security::SecurityAssessment;
use crate::state::amt::{net_signed, parse_u256, u256_dec};
use crate::state::lifecycle::TokenLifecycleState;
use crate::state::market::{MarketState, MarketStateQuality};
use crate::state::rolling::RollingWindowSnapshot;
use crate::state::TokenStateSnapshot;

use super::opt::{count_ratio_bps, imbalance_i64, FeatureQuality, OptAmt, OptI64, OptU64};
use super::vector::{
    FeatureVector, ProtocolFeatures, SharedFeatures, WindowFeatures, FEATURE_VERSION,
};

/// Adjacent-window lookbacks used for velocity/acceleration. Never look forward.
pub const ACCEL_LOOKBACK_5S_MS: i64 = 5_000;
pub const ACCEL_LOOKBACK_15S_MS: i64 = 15_000;
pub const ACCEL_LOOKBACK_30S_MS: i64 = 30_000;
pub const ACCEL_LOOKBACK_60S_MS: i64 = 60_000;

pub struct FeatureInput<'a> {
    pub snapshot: &'a TokenStateSnapshot,
    /// Snapshot taken ~15s earlier so trailing 15s windows do not overlap.
    pub prior_15s: Option<&'a TokenStateSnapshot>,
    pub prior_5s: Option<&'a TokenStateSnapshot>,
    pub prior_30s: Option<&'a TokenStateSnapshot>,
    pub prior_60s: Option<&'a TokenStateSnapshot>,
    pub security: Option<&'a SecurityAssessment>,
}

impl<'a> FeatureInput<'a> {
    pub fn from_history(
        snapshot: &'a TokenStateSnapshot,
        history: &'a [TokenStateSnapshot],
        security: Option<&'a SecurityAssessment>,
    ) -> Self {
        Self {
            snapshot,
            prior_5s: prior_at_or_before(history, snapshot, ACCEL_LOOKBACK_5S_MS),
            prior_15s: prior_at_or_before(history, snapshot, ACCEL_LOOKBACK_15S_MS),
            prior_30s: prior_at_or_before(history, snapshot, ACCEL_LOOKBACK_30S_MS),
            prior_60s: prior_at_or_before(history, snapshot, ACCEL_LOOKBACK_60S_MS),
            security,
        }
    }
}

/// Latest same-token snapshot with `snapshot_time <= current.snapshot_time - lookback_ms`.
pub fn prior_at_or_before<'a>(
    history: &'a [TokenStateSnapshot],
    current: &TokenStateSnapshot,
    lookback_ms: i64,
) -> Option<&'a TokenStateSnapshot> {
    let cutoff = current.snapshot_time - chrono::Duration::milliseconds(lookback_ms);
    history
        .iter()
        .filter(|s| {
            s.chain == current.chain
                && s.token_address == current.token_address
                && s.snapshot_time <= cutoff
                && s.snapshot_time <= current.snapshot_time
        })
        .max_by_key(|s| s.snapshot_time)
}

pub struct FeatureEngine;

impl FeatureEngine {
    pub fn compute(input: FeatureInput<'_>) -> FeatureVector {
        let s = input.snapshot;
        let w5 = &s.rolling_5s;
        let w15 = &s.rolling_15s;
        let w30 = &s.rolling_30s;
        let w60 = &s.rolling_60s;
        let trades = s.buy_count_total.saturating_add(s.sell_count_total);
        let avg_buy = avg_size(s.buy_count_total, &s.buy_quote_volume_raw_total);
        let avg_sell = avg_size(s.sell_count_total, &s.sell_quote_volume_raw_total);
        let observed_vol = crate::state::amt::add_raw(
            &s.buy_quote_volume_raw_total,
            &s.sell_quote_volume_raw_total,
        );
        let creator_frac = amt_ratio_bps(&s.creator_sell_quote_raw, &observed_vol);

        let buyer_accel_5 = accel_unique_buyers(w5, input.prior_5s.map(|p| &p.rolling_5s));
        let buyer_accel_15 = accel_unique_buyers(w15, input.prior_15s.map(|p| &p.rolling_15s));
        let buyer_accel_30 = accel_unique_buyers(w30, input.prior_30s.map(|p| &p.rolling_30s));
        let seller_accel_15 = accel_unique_sellers(w15, input.prior_15s.map(|p| &p.rolling_15s));
        let seller_accel_30 = accel_unique_sellers(w30, input.prior_30s.map(|p| &p.rolling_30s));

        let buy_vol_vel = volume_delta(
            &w15.buy_quote_volume_raw,
            input
                .prior_15s
                .map(|p| p.rolling_15s.buy_quote_volume_raw.as_str()),
        );
        let sell_vol_vel = volume_delta(
            &w15.sell_quote_volume_raw,
            input
                .prior_15s
                .map(|p| p.rolling_15s.sell_quote_volume_raw.as_str()),
        );
        let net_vel = net_delta(
            &w15.net_quote_flow,
            input
                .prior_15s
                .map(|p| p.rolling_15s.net_quote_flow.as_str()),
        );

        let liq_q = liquidity_from_market(&s.market_state);
        let liq_quality = match s.market_state.quality() {
            MarketStateQuality::Complete => FeatureQuality::Value,
            MarketStateQuality::Partial => FeatureQuality::Partial,
            MarketStateQuality::Unknown => FeatureQuality::Unknown,
        };

        let (pre, mig, gap, post) = grad_flags(s.lifecycle_state);
        let sec = input.security;
        let hour = s.snapshot_time.hour();
        let wlt = &s.wallet;

        let unique_traders = match wlt.unique_traders_total {
            Some(v) => OptU64::value(v),
            None => OptU64::unknown(),
        };

        let tps = |count: u64, ms: i64| -> OptU64 {
            if ms <= 0 {
                return OptU64::unknown();
            }
            OptU64::value(count.saturating_mul(1000) / ms as u64)
        };

        let price_now = price_from_last(s);
        let price_change = |prior: Option<&TokenStateSnapshot>| match prior {
            Some(p) => price_change_bps(&price_now, &price_from_last(p)),
            None => OptI64::unknown(),
        };

        let shared = SharedFeatures {
            token_age_ms: s.age_ms,
            trade_count_total: trades,
            buy_count_total: s.buy_count_total,
            sell_count_total: s.sell_count_total,
            unique_buyers_total: s.unique_buyers_total,
            unique_sellers_total: s.unique_sellers_total,
            unique_traders_total: unique_traders.clone(),
            buy_quote_volume_total: s.buy_quote_volume_raw_total.clone(),
            sell_quote_volume_total: s.sell_quote_volume_raw_total.clone(),
            net_quote_flow_total: net_signed(
                &s.buy_quote_volume_raw_total,
                &s.sell_quote_volume_raw_total,
            ),
            avg_buy_size: avg_buy,
            median_buy_size: w60
                .trade_size_median
                .clone()
                .map(OptAmt::value)
                .unwrap_or_else(OptAmt::unknown),
            max_buy_size: w60
                .trade_size_max
                .clone()
                .map(OptAmt::value)
                .unwrap_or_else(OptAmt::unknown),
            avg_sell_size: avg_sell,
            median_sell_size: OptAmt::unknown(),
            max_sell_size: OptAmt::unknown(),
            creator_buy_quote_total: s.creator_buy_quote_raw.clone(),
            creator_sell_quote_total: s.creator_sell_quote_raw.clone(),
            creator_net_quote_flow: net_signed(&s.creator_buy_quote_raw, &s.creator_sell_quote_raw),
            creator_buy_count: s.creator_buy_count,
            creator_sell_count: s.creator_sell_count,
            creator_has_sold: s.creator_sell_count > 0,
            creator_sell_fraction_bps: creator_frac,
            time_since_last_trade_ms: opt_i64(wlt.last_trade_age_ms),
            time_since_last_buy_ms: opt_i64(wlt.last_buy_age_ms),
            time_since_last_sell_ms: opt_i64(wlt.last_sell_age_ms),
            time_since_creator_last_sell_ms: opt_i64(wlt.creator_last_sell_age_ms),
            trade_count_imbalance: imbalance_i64(s.buy_count_total, s.sell_count_total),
            buy_sell_count_ratio_bps: count_ratio_bps(s.buy_count_total, s.sell_count_total),
            quote_volume_imbalance: net_signed(
                &s.buy_quote_volume_raw_total,
                &s.sell_quote_volume_raw_total,
            ),
            buy_sell_quote_ratio_bps: amt_ratio_bps(
                &s.buy_quote_volume_raw_total,
                &s.sell_quote_volume_raw_total,
            ),
            unique_buyer_seller_ratio_bps: count_ratio_bps(
                s.unique_buyers_total,
                s.unique_sellers_total,
            ),
            win5: window_feats(w5),
            win15: window_feats(w15),
            win30: window_feats(w30),
            win60: window_feats(w60),
            unique_buyer_velocity_5s: OptI64::value(w5.unique_buyers as i64),
            unique_buyer_velocity_15s: OptI64::value(w15.unique_buyers as i64),
            unique_buyer_acceleration_5s: buyer_accel_5,
            unique_buyer_acceleration_15s: buyer_accel_15,
            unique_buyer_acceleration_30s: buyer_accel_30,
            unique_seller_velocity_5s: OptI64::value(w5.unique_sellers as i64),
            unique_seller_velocity_15s: OptI64::value(w15.unique_sellers as i64),
            unique_seller_acceleration_15s: seller_accel_15,
            unique_seller_acceleration_30s: seller_accel_30,
            buy_volume_velocity_15s: buy_vol_vel,
            sell_volume_velocity_15s: sell_vol_vel,
            net_flow_velocity_15s: net_vel,
            trades_per_second_5s_milli: tps(w5.buy_count + w5.sell_count, 5_000),
            trades_per_second_15s_milli: tps(w15.buy_count + w15.sell_count, 15_000),
            trades_per_second_60s_milli: tps(w60.buy_count + w60.sell_count, 60_000),
            buy_trades_per_second_15s_milli: tps(w15.buy_count, 15_000),
            sell_trades_per_second_15s_milli: tps(w15.sell_count, 15_000),
            repeat_buyer_count: opt_u64(wlt.repeat_buyer_count),
            repeat_buyer_ratio_bps: wlt
                .repeat_buyer_count
                .and_then(|r| count_ratio_bps(r, s.unique_buyers_total)),
            mean_buys_per_buyer_milli: opt_u64(wlt.mean_buys_per_buyer_milli),
            median_buys_per_buyer: opt_u64(wlt.median_buys_per_buyer),
            new_buyer_ratio_30s_bps: count_ratio_bps(w30.new_unique_buyers, w30.unique_buyers),
            trades_per_unique_wallet_milli: match unique_traders.as_value() {
                Some(u) if u > 0 => OptU64::value(trades.saturating_mul(1000) / u),
                Some(0) if trades == 0 => OptU64::value(0),
                _ => OptU64::unknown(),
            },
            top_trader_trade_share_bps: opt_u64(wlt.top_trader_trade_share_bps.map(u64::from)),
            top_trader_volume_share_bps: opt_u64(wlt.top_trader_volume_share_bps.map(u64::from)),
            wash_trade_indicator_count: OptU64::unknown(),
            wash_trade_volume_fraction_bps: OptU64::unknown(),
            wash_adjustment_quality: FeatureQuality::Unknown,
            holder_count: OptU64::unknown(),
            top1_pct_bps: OptU64::unknown(),
            top5_pct_bps: OptU64::unknown(),
            top10_pct_bps: OptU64::unknown(),
            top20_pct_bps: OptU64::unknown(),
            creator_pct_bps: OptU64::unknown(),
            cluster_merged_top10_pct_bps: OptU64::unknown(),
            bundle_supply_pct_bps: OptU64::unknown(),
            creator_cluster_supply_pct_bps: OptU64::unknown(),
            liquidity_quote: liq_q,
            liquidity_quality: liq_quality,
            estimated_exit_capacity: OptAmt::unknown(),
            max_notional_at_1pct: OptAmt::unknown(),
            max_notional_at_2pct: OptAmt::unknown(),
            max_notional_at_5pct: OptAmt::unknown(),
            current_price_quote_per_token: price_now.clone(),
            price_change_5s_bps: price_change(input.prior_5s),
            price_change_15s_bps: price_change(input.prior_15s),
            price_change_30s_bps: price_change(input.prior_30s),
            price_change_60s_bps: price_change(input.prior_60s),
            return_since_discovery_bps: OptI64::unknown(),
            is_pre_graduation: pre,
            is_migrating: mig,
            is_graduation_gap: gap,
            is_post_graduation: post,
            time_since_graduation_ms: graduation_age(s),
            current_progress_to_graduation_bps: s
                .graduation_progress_bps
                .or(s.curve_progress_bps)
                .map(|v| OptU64::value(u64::from(v)))
                .unwrap_or_else(OptU64::unknown),
            security_verdict: sec.map(|a| a.verdict.as_str().to_string()),
            contract_risk: sec.map(|a| a.contract_risk.as_str().to_string()),
            privilege_risk: sec.map(|a| a.privilege_risk.as_str().to_string()),
            sellability_risk: sec.map(|a| a.sellability_risk.as_str().to_string()),
            liquidity_structure_risk: sec.map(|a| a.liquidity_structure_risk.as_str().to_string()),
            warning_count: sec.map(|a| a.warnings.len() as u32).unwrap_or(0),
            creator_prior_launches: OptU64::unknown(),
            creator_prior_rugs: OptU64::unknown(),
            hour_of_day_utc: hour,
            launches_last_5m: OptU64::unknown(),
        };

        let mut v = FeatureVector {
            id: None,
            chain: s.chain,
            token_address: s.token_address.clone(),
            launchpad: s.launchpad,
            snapshot_id: s.id,
            security_assessment_id: sec.and_then(|a| a.id),
            as_of_block: s.as_of_block,
            as_of_block_hash: None,
            as_of_slot: s.as_of_slot,
            as_of_time: s.snapshot_time,
            token_age_ms: s.age_ms,
            feature_version: FEATURE_VERSION.into(),
            data_quality: s.data_quality,
            flow_quality: FeatureQuality::Value,
            liquidity_quality: liq_quality,
            holder_quality: FeatureQuality::Unknown,
            creator_quality: FeatureQuality::Unknown,
            shared,
            protocol: protocol_feats(s),
            fingerprint: String::new(),
            created_at: Utc::now(),
        };
        v.fingerprint = v.content_fingerprint();
        v
    }
}

/// unique_buyers(T-W, T] − unique_buyers(T-2W, T-W]. Prior window must already be history.
fn accel_unique_buyers(
    cur: &RollingWindowSnapshot,
    prior: Option<&RollingWindowSnapshot>,
) -> OptI64 {
    match prior {
        Some(p) => OptI64::value(cur.unique_buyers as i64 - p.unique_buyers as i64),
        None => OptI64::unknown(),
    }
}

fn accel_unique_sellers(
    cur: &RollingWindowSnapshot,
    prior: Option<&RollingWindowSnapshot>,
) -> OptI64 {
    match prior {
        Some(p) => OptI64::value(cur.unique_sellers as i64 - p.unique_sellers as i64),
        None => OptI64::unknown(),
    }
}

fn volume_delta(cur: &str, prior: Option<&str>) -> OptAmt {
    match prior {
        Some(p) => OptAmt::value(net_signed(cur, p)),
        None => OptAmt::unknown(),
    }
}

fn net_delta(cur: &str, prior: Option<&str>) -> OptAmt {
    volume_delta(cur, prior)
}

fn window_feats(w: &RollingWindowSnapshot) -> WindowFeatures {
    WindowFeatures {
        duration_ms: w.duration_ms,
        buy_count: w.buy_count,
        sell_count: w.sell_count,
        unique_buyers: w.unique_buyers,
        unique_sellers: w.unique_sellers,
        new_unique_buyers: w.new_unique_buyers,
        new_unique_sellers: w.new_unique_sellers,
        buy_quote_volume: empty_zero(&w.buy_quote_volume_raw),
        sell_quote_volume: empty_zero(&w.sell_quote_volume_raw),
        net_quote_flow: empty_zero(&w.net_quote_flow),
        median_trade_size: w
            .trade_size_median
            .clone()
            .map(OptAmt::value)
            .unwrap_or_else(OptAmt::unknown),
        max_trade_size: w
            .trade_size_max
            .clone()
            .map(OptAmt::value)
            .unwrap_or_else(OptAmt::unknown),
        creator_buy_volume: empty_zero(&w.creator_buy_volume),
        creator_sell_volume: empty_zero(&w.creator_sell_volume),
        trade_count_imbalance: imbalance_i64(w.buy_count, w.sell_count),
        buy_sell_count_ratio_bps: count_ratio_bps(w.buy_count, w.sell_count),
    }
}

fn empty_zero(s: &str) -> String {
    if s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

fn avg_size(count: u64, sum: &str) -> Option<String> {
    if count == 0 {
        return None;
    }
    Some(u256_dec(
        parse_u256(sum) / alloy_primitives::U256::from(count),
    ))
}

fn amt_ratio_bps(n: &str, d: &str) -> Option<u32> {
    let d = parse_u256(d);
    if d.is_zero() {
        return None;
    }
    let n = parse_u256(n);
    Some(
        (n.saturating_mul(alloy_primitives::U256::from(10_000u64)) / d)
            .try_into()
            .unwrap_or(u32::MAX),
    )
}

fn opt_u64(v: Option<u64>) -> OptU64 {
    v.map(OptU64::value).unwrap_or_else(OptU64::unknown)
}

fn opt_i64(v: Option<i64>) -> OptI64 {
    v.map(OptI64::value).unwrap_or_else(OptI64::unknown)
}

fn price_from_last(s: &TokenStateSnapshot) -> OptAmt {
    match (&s.last_trade_quote_raw, &s.last_trade_token_raw) {
        (Some(q), Some(t)) if parse_u256(t) > alloy_primitives::U256::ZERO => {
            let p = parse_u256(q).saturating_mul(
                alloy_primitives::U256::from(10u64).pow(alloy_primitives::U256::from(18u64)),
            ) / parse_u256(t);
            OptAmt::value(u256_dec(p))
        }
        _ => OptAmt::unknown(),
    }
}

fn price_change_bps(now: &OptAmt, prior: &OptAmt) -> OptI64 {
    let (OptAmt::Value { v: n }, OptAmt::Value { v: p }) = (now, prior) else {
        return OptI64::unknown();
    };
    let pd = parse_u256(p);
    if pd.is_zero() {
        return OptI64::unknown();
    }
    let nd = parse_u256(n);
    if nd >= pd {
        let bps = (nd - pd).saturating_mul(alloy_primitives::U256::from(10_000u64)) / pd;
        OptI64::value(i64::try_from(bps).unwrap_or(i64::MAX))
    } else {
        let bps = (pd - nd).saturating_mul(alloy_primitives::U256::from(10_000u64)) / pd;
        OptI64::value(-i64::try_from(bps).unwrap_or(i64::MAX))
    }
}

fn liquidity_from_market(m: &MarketState) -> OptAmt {
    match m {
        MarketState::BondingCurve(b) => b
            .real_sol_reserves
            .clone()
            .map(OptAmt::value)
            .unwrap_or_else(OptAmt::unknown),
        MarketState::ConstantProduct(c) => match &c.reserve_quote_raw {
            Some(v) => OptAmt::Partial { v: Some(v.clone()) },
            None => OptAmt::unknown(),
        },
        MarketState::UniswapV4(u) => u
            .liquidity_raw
            .clone()
            .map(OptAmt::value)
            .unwrap_or_else(OptAmt::unknown),
        MarketState::Unknown => OptAmt::unknown(),
    }
}

fn grad_flags(ls: TokenLifecycleState) -> (bool, bool, bool, bool) {
    match ls {
        TokenLifecycleState::Discovered | TokenLifecycleState::CurveActive => {
            (true, false, false, false)
        }
        TokenLifecycleState::MigrationPending | TokenLifecycleState::Migrating => {
            (false, true, false, false)
        }
        TokenLifecycleState::LaunchSwept | TokenLifecycleState::GraduationGap => {
            (false, false, true, false)
        }
        TokenLifecycleState::AmmActive => (false, false, false, true),
        _ => (false, false, false, false),
    }
}

fn graduation_age(_s: &TokenStateSnapshot) -> OptI64 {
    // Graduation time is only known retrospectively; never infer future migrate time.
    OptI64::unknown()
}

fn protocol_feats(s: &TokenStateSnapshot) -> ProtocolFeatures {
    match s.launchpad {
        Launchpad::PumpFun => match &s.market_state {
            MarketState::BondingCurve(b) => ProtocolFeatures::SolanaPump {
                curve_progress_bps: s
                    .curve_progress_bps
                    .map(|v| OptU64::value(u64::from(v)))
                    .unwrap_or_else(OptU64::unknown),
                virtual_quote_reserve: b
                    .virtual_sol_reserves
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
                real_quote_reserve: b
                    .real_sol_reserves
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
                token_reserve: b
                    .real_token_reserves
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
                curve_progress_velocity_bps: OptI64::unknown(),
            },
            _ => ProtocolFeatures::SolanaPump {
                curve_progress_bps: s
                    .curve_progress_bps
                    .map(|v| OptU64::value(u64::from(v)))
                    .unwrap_or_else(OptU64::unknown),
                virtual_quote_reserve: OptAmt::unknown(),
                real_quote_reserve: OptAmt::unknown(),
                token_reserve: OptAmt::unknown(),
                curve_progress_velocity_bps: OptI64::unknown(),
            },
        },
        Launchpad::PonsV2 => ProtocolFeatures::RobinhoodPons {
            graduation_progress_bps: s
                .graduation_progress_bps
                .map(|v| OptU64::value(u64::from(v)))
                .unwrap_or_else(OptU64::unknown),
            snipe_tax_window_elapsed: OptU64::unknown(),
        },
        Launchpad::ClankerV4 => match &s.market_state {
            MarketState::UniswapV4(u) => ProtocolFeatures::BaseClanker {
                has_pool_id: u.pool_id.is_some(),
                sqrt_price_x96: u
                    .sqrt_price_x96
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
                liquidity_raw: u
                    .liquidity_raw
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
                tick: u
                    .tick
                    .clone()
                    .map(OptAmt::value)
                    .unwrap_or_else(OptAmt::unknown),
            },
            _ => ProtocolFeatures::BaseClanker {
                has_pool_id: false,
                sqrt_price_x96: OptAmt::unknown(),
                liquidity_raw: OptAmt::unknown(),
                tick: OptAmt::unknown(),
            },
        },
        _ => ProtocolFeatures::None,
    }
}

pub fn get_feature_at_or_before(
    rows: &[FeatureVector],
    time: DateTime<Utc>,
) -> Option<&FeatureVector> {
    rows.iter()
        .filter(|f| f.as_of_time <= time)
        .max_by_key(|f| f.as_of_time)
}

pub fn latest_security_at<'a>(
    rows: &'a [SecurityAssessment],
    chain: crate::domain::Chain,
    token: &str,
    time: DateTime<Utc>,
) -> Option<&'a SecurityAssessment> {
    rows.iter()
        .filter(|a| a.chain == chain && a.token_address == token && a.as_of_time <= time)
        .max_by_key(|a| a.as_of_time)
}
