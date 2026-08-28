//! Venue impact. Bonding-curve and constant-product are modelled. Uni v4 is not faked as CP.

use alloy_primitives::U256;

use crate::domain::Launchpad;
use crate::state::amt::{parse_u256, u256_dec};
use crate::state::lifecycle::TokenLifecycleState;
use crate::state::market::{MarketState, MarketStateQuality};
use crate::state::TokenStateSnapshot;

use super::models::{apply_adverse_bps, FeeModel, SlippageModel};
use super::types::{ExecutionQuality, ExecutionStatus, OrderSide};

const SCALE: u32 = 18;

#[derive(Debug, Clone)]
pub struct VenueFill {
    pub status: ExecutionStatus,
    pub quality: ExecutionQuality,
    pub token_amount: String,
    pub quote_amount: String,
    pub protocol_fee: String,
    pub snipe_tax: String,
    pub reference_price_1e18: String,
    pub effective_price_1e18: String,
    pub price_impact_bps: Option<u32>,
    pub fill_fraction_bps: u32,
    pub reason: Option<String>,
}

impl VenueFill {
    fn fail(status: ExecutionStatus, quality: ExecutionQuality, reason: impl Into<String>) -> Self {
        Self {
            status,
            quality,
            token_amount: "0".into(),
            quote_amount: "0".into(),
            protocol_fee: "0".into(),
            snipe_tax: "0".into(),
            reference_price_1e18: "0".into(),
            effective_price_1e18: "0".into(),
            price_impact_bps: None,
            fill_fraction_bps: 0,
            reason: Some(reason.into()),
        }
    }
}

pub fn executable_fill(
    snap: &TokenStateSnapshot,
    side: OrderSide,
    quote_or_token: &str,
    fees: &FeeModel,
    slip: &SlippageModel,
    max_slippage_bps: u32,
    force_snipe: bool,
) -> VenueFill {
    if matches!(
        snap.lifecycle_state,
        TokenLifecycleState::GraduationGap | TokenLifecycleState::LaunchSwept
    ) {
        return VenueFill::fail(
            ExecutionStatus::TemporarilyUnavailable,
            ExecutionQuality::Modelled,
            "PONS_GRADUATION_GAP",
        );
    }
    if matches!(
        snap.lifecycle_state,
        TokenLifecycleState::Inactive | TokenLifecycleState::Migrating
    ) && side == OrderSide::Sell
    {
        return VenueFill::fail(
            ExecutionStatus::RejectedLiquidity,
            ExecutionQuality::Modelled,
            "MARKET_NOT_SELLABLE",
        );
    }

    let snipe = fees.snipe_tax_bps(snap.launchpad, snap.age_ms, force_snipe);
    match &snap.market_state {
        MarketState::BondingCurve(c) => {
            let (vs, vt) = match (&c.virtual_sol_reserves, &c.virtual_token_reserves) {
                (Some(s), Some(t)) if parse_u256(s) > U256::ZERO && parse_u256(t) > U256::ZERO => {
                    (s.clone(), t.clone())
                }
                _ => {
                    return VenueFill::fail(
                        ExecutionStatus::UnavailableMarketState,
                        ExecutionQuality::NonResearchValid,
                        "UNKNOWN_CURVE_RESERVES",
                    );
                }
            };
            let fee_bps = fees.protocol_bps(snap.launchpad, snap.lifecycle_state);
            let q = match c.quality {
                MarketStateQuality::Complete => ExecutionQuality::Exact,
                MarketStateQuality::Partial => ExecutionQuality::Modelled,
                MarketStateQuality::Unknown => ExecutionQuality::NonResearchValid,
            };
            if q == ExecutionQuality::NonResearchValid {
                return VenueFill::fail(
                    ExecutionStatus::UnavailableMarketState,
                    q,
                    "UNKNOWN_CURVE_QUALITY",
                );
            }
            curve_swap(
                &vs,
                &vt,
                c.real_token_reserves.as_deref(),
                c.real_sol_reserves.as_deref(),
                side,
                quote_or_token,
                fee_bps,
                snipe,
                slip.adverse_bps,
                max_slippage_bps,
                q,
            )
        }
        MarketState::ConstantProduct(c) => {
            let (rq, rt) = match (&c.reserve_quote_raw, &c.reserve_token_raw) {
                (Some(q), Some(t)) if parse_u256(q) > U256::ZERO && parse_u256(t) > U256::ZERO => {
                    (q.clone(), t.clone())
                }
                _ => {
                    return VenueFill::fail(
                        ExecutionStatus::UnavailableMarketState,
                        ExecutionQuality::NonResearchValid,
                        "UNKNOWN_CP_RESERVES",
                    );
                }
            };
            if c.quality == MarketStateQuality::Unknown {
                return VenueFill::fail(
                    ExecutionStatus::UnavailableMarketState,
                    ExecutionQuality::NonResearchValid,
                    "UNKNOWN_CP_QUALITY",
                );
            }
            let fee_bps = fees.protocol_bps(
                if snap.launchpad == Launchpad::PumpFun {
                    Launchpad::PumpSwap
                } else {
                    snap.launchpad
                },
                snap.lifecycle_state,
            );
            let q = match c.quality {
                MarketStateQuality::Complete => ExecutionQuality::Exact,
                MarketStateQuality::Partial => ExecutionQuality::PartialState,
                MarketStateQuality::Unknown => ExecutionQuality::NonResearchValid,
            };
            cp_swap(
                &rq,
                &rt,
                side,
                quote_or_token,
                fee_bps,
                snipe,
                slip.adverse_bps,
                max_slippage_bps,
                q,
            )
        }
        MarketState::UniswapV4(_) => VenueFill::fail(
            ExecutionStatus::UnavailableMarketState,
            ExecutionQuality::PartialState,
            "IMPACT_MODEL_PARTIAL_UNISWAP_V4",
        ),
        MarketState::Unknown => VenueFill::fail(
            ExecutionStatus::UnavailableMarketState,
            ExecutionQuality::NonResearchValid,
            "UNKNOWN_LIQUIDITY_NOT_INFINITE",
        ),
    }
}

fn scale() -> U256 {
    U256::from(10u64).pow(U256::from(SCALE))
}

fn price_1e18(quote: &U256, token: &U256) -> String {
    if token.is_zero() {
        return "0".into();
    }
    u256_dec(quote.saturating_mul(scale()) / token)
}

fn impact_bps(ref_px: U256, eff_px: U256, side: OrderSide) -> Option<u32> {
    if ref_px.is_zero() {
        return None;
    }
    let (hi, lo) = match side {
        OrderSide::Buy => {
            if eff_px >= ref_px {
                (eff_px, ref_px)
            } else {
                (ref_px, ref_px)
            }
        }
        OrderSide::Sell => {
            if ref_px >= eff_px {
                (ref_px, eff_px)
            } else {
                (ref_px, ref_px)
            }
        }
    };
    let bps = (hi - lo).saturating_mul(U256::from(10_000u64)) / ref_px;
    Some(u32::try_from(bps).unwrap_or(u32::MAX))
}

#[allow(clippy::too_many_arguments)]
fn curve_swap(
    v_sol: &str,
    v_tok: &str,
    real_tok: Option<&str>,
    real_sol: Option<&str>,
    side: OrderSide,
    amount: &str,
    fee_bps: u32,
    snipe_bps: u32,
    adverse_bps: u32,
    max_slip_bps: u32,
    quality: ExecutionQuality,
) -> VenueFill {
    let vs = parse_u256(v_sol);
    let vt = parse_u256(v_tok);
    if vs.is_zero() || vt.is_zero() {
        return VenueFill::fail(
            ExecutionStatus::RejectedLiquidity,
            quality,
            "ZERO_VIRTUAL_RESERVE",
        );
    }
    let k = vs.saturating_mul(vt);
    let ref_px = parse_u256(&price_1e18(&vs, &vt));
    match side {
        OrderSide::Buy => {
            let quote_in = parse_u256(amount);
            if quote_in.is_zero() {
                return VenueFill::fail(ExecutionStatus::NoFill, quality, "ZERO_QUOTE");
            }
            let tax = quote_in.saturating_mul(U256::from(snipe_bps)) / U256::from(10_000u64);
            let after_tax = quote_in.saturating_sub(tax);
            let proto = after_tax.saturating_mul(U256::from(fee_bps)) / U256::from(10_000u64);
            let net = after_tax.saturating_sub(proto);
            if net.is_zero() {
                return VenueFill::fail(
                    ExecutionStatus::RejectedSlippage,
                    quality,
                    "FEE_TAX_CONSUMED_QUOTE",
                );
            }
            let new_sol = vs.saturating_add(net);
            let new_tok = k / new_sol;
            let uncapped = vt.saturating_sub(new_tok);
            let mut tok_out = uncapped;
            if let Some(rt) = real_tok {
                let cap = parse_u256(rt);
                if !cap.is_zero() && tok_out > cap {
                    tok_out = cap;
                }
            }
            tok_out = parse_u256(&apply_adverse_bps(&u256_dec(tok_out), adverse_bps));
            if tok_out.is_zero() {
                return VenueFill::fail(ExecutionStatus::NoFill, quality, "ZERO_TOKEN_OUT");
            }
            let filled_quote = if tok_out < uncapped && tok_out > U256::ZERO {
                let new_t = vt.saturating_sub(tok_out);
                if new_t.is_zero() {
                    quote_in
                } else {
                    (k / new_t).saturating_sub(vs)
                }
            } else {
                quote_in
            };
            let eff = parse_u256(&price_1e18(&filled_quote.max(net), &tok_out));
            let imp = impact_bps(ref_px, eff, OrderSide::Buy).unwrap_or(0);
            if imp > max_slip_bps {
                return VenueFill::fail(
                    ExecutionStatus::RejectedSlippage,
                    quality,
                    format!("IMPACT_{imp}_GT_MAX_{max_slip_bps}"),
                );
            }
            let frac = if uncapped.is_zero() {
                10_000
            } else {
                u32::try_from(tok_out.saturating_mul(U256::from(10_000u64)) / uncapped)
                    .unwrap_or(10_000)
            };
            let status = if tok_out < uncapped {
                ExecutionStatus::PartialFill
            } else if frac >= 10_000 {
                ExecutionStatus::Filled
            } else if frac > 0 {
                ExecutionStatus::PartialFill
            } else {
                ExecutionStatus::NoFill
            };
            VenueFill {
                status,
                quality,
                token_amount: u256_dec(tok_out),
                quote_amount: u256_dec(filled_quote),
                protocol_fee: u256_dec(proto),
                snipe_tax: u256_dec(tax),
                reference_price_1e18: u256_dec(ref_px),
                effective_price_1e18: u256_dec(eff),
                price_impact_bps: Some(imp),
                fill_fraction_bps: frac.min(10_000),
                reason: None,
            }
        }
        OrderSide::Sell => {
            let tok_in = parse_u256(amount);
            if tok_in.is_zero() {
                return VenueFill::fail(ExecutionStatus::NoFill, quality, "ZERO_TOKEN");
            }
            let new_tok = vt.saturating_add(tok_in);
            let new_sol = k / new_tok;
            let mut quote_out = vs.saturating_sub(new_sol);
            if let Some(rs) = real_sol {
                let cap = parse_u256(rs);
                if !cap.is_zero() && quote_out > cap {
                    quote_out = cap;
                }
            }
            let proto = quote_out.saturating_mul(U256::from(fee_bps)) / U256::from(10_000u64);
            quote_out = quote_out.saturating_sub(proto);
            let tax = quote_out.saturating_mul(U256::from(snipe_bps)) / U256::from(10_000u64);
            quote_out = quote_out.saturating_sub(tax);
            quote_out = parse_u256(&apply_adverse_bps(&u256_dec(quote_out), adverse_bps));
            if quote_out.is_zero() {
                return VenueFill::fail(ExecutionStatus::NoFill, quality, "ZERO_QUOTE_OUT");
            }
            let filled_tok = tok_in;
            let eff = parse_u256(&price_1e18(&quote_out, &filled_tok));
            let imp = impact_bps(ref_px, eff, OrderSide::Sell).unwrap_or(0);
            if imp > max_slip_bps {
                return VenueFill::fail(
                    ExecutionStatus::RejectedSlippage,
                    quality,
                    format!("IMPACT_{imp}_GT_MAX_{max_slip_bps}"),
                );
            }
            VenueFill {
                status: ExecutionStatus::Filled,
                quality,
                token_amount: u256_dec(filled_tok),
                quote_amount: u256_dec(quote_out),
                protocol_fee: u256_dec(proto),
                snipe_tax: u256_dec(tax),
                reference_price_1e18: u256_dec(ref_px),
                effective_price_1e18: u256_dec(eff),
                price_impact_bps: Some(imp),
                fill_fraction_bps: 10_000,
                reason: None,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cp_swap(
    r_quote: &str,
    r_tok: &str,
    side: OrderSide,
    amount: &str,
    fee_bps: u32,
    snipe_bps: u32,
    adverse_bps: u32,
    max_slip_bps: u32,
    quality: ExecutionQuality,
) -> VenueFill {
    curve_swap(
        r_quote,
        r_tok,
        Some(r_tok),
        Some(r_quote),
        side,
        amount,
        fee_bps,
        snipe_bps,
        adverse_bps,
        max_slip_bps,
        quality,
    )
}

/// Max quote notional whose buy impact is <= `impact_bps`. UNKNOWN if no reserves.
pub fn max_quote_at_impact(snap: &TokenStateSnapshot, impact_bps: u32) -> Option<String> {
    let (vs, vt) = match &snap.market_state {
        MarketState::BondingCurve(c) => (
            c.virtual_sol_reserves.as_ref()?.clone(),
            c.virtual_token_reserves.as_ref()?.clone(),
        ),
        MarketState::ConstantProduct(c) => (
            c.reserve_quote_raw.as_ref()?.clone(),
            c.reserve_token_raw.as_ref()?.clone(),
        ),
        _ => return None,
    };
    let vs = parse_u256(&vs);
    let vt = parse_u256(&vt);
    if vs.is_zero() || vt.is_zero() {
        return None;
    }
    let fees = FeeModel::research_default();
    let slip = SlippageModel::none();
    let mut lo = U256::from(1u64);
    let mut hi = vs / U256::from(2u64);
    if hi.is_zero() {
        hi = vs;
    }
    let mut best = U256::ZERO;
    for _ in 0..64 {
        if lo > hi {
            break;
        }
        let mid = lo + (hi - lo) / U256::from(2u64);
        let fill = curve_swap(
            &u256_dec(vs),
            &u256_dec(vt),
            None,
            None,
            OrderSide::Buy,
            &u256_dec(mid),
            fees.pump_curve_protocol_bps,
            0,
            0,
            u32::MAX,
            ExecutionQuality::Modelled,
        );
        let ok = fill.status.is_fill() && fill.price_impact_bps.unwrap_or(u32::MAX) <= impact_bps;
        if ok {
            best = mid;
            lo = mid + U256::from(1u64);
        } else {
            if mid.is_zero() {
                break;
            }
            hi = mid - U256::from(1u64);
        }
        let _ = slip;
    }
    if best.is_zero() {
        None
    } else {
        Some(u256_dec(best))
    }
}

pub fn mark_exit_quote(
    snap: &TokenStateSnapshot,
    token_amount: &str,
    fees: &FeeModel,
) -> Option<String> {
    let fill = executable_fill(
        snap,
        OrderSide::Sell,
        token_amount,
        fees,
        &SlippageModel::none(),
        u32::MAX,
        false,
    );
    if fill.status.is_fill() {
        Some(fill.quote_amount)
    } else {
        None
    }
}

pub fn spot_price_1e18(snap: &TokenStateSnapshot) -> Option<String> {
    match &snap.market_state {
        MarketState::BondingCurve(c) => Some(price_1e18(
            &parse_u256(c.virtual_sol_reserves.as_ref()?),
            &parse_u256(c.virtual_token_reserves.as_ref()?),
        )),
        MarketState::ConstantProduct(c) => Some(price_1e18(
            &parse_u256(c.reserve_quote_raw.as_ref()?),
            &parse_u256(c.reserve_token_raw.as_ref()?),
        )),
        _ => {
            let (Some(q), Some(t)) = (&snap.last_trade_quote_raw, &snap.last_trade_token_raw)
            else {
                return None;
            };
            let t = parse_u256(t);
            if t.is_zero() {
                return None;
            }
            Some(price_1e18(&parse_u256(q), &t))
        }
    }
}
