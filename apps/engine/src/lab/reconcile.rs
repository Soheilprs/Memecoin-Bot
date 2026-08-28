//! Rebuild position inventory and realized PnL from persisted fills.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};

use crate::sim::position::SimulatedPosition;
use crate::sim::types::PositionStatus;
use crate::state::amt::{parse_u256, u256_dec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillLeg {
    pub token: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciledPosition {
    pub position_id: i64,
    pub token: String,
    pub strategy_policy_id: String,
    pub entry_token: String,
    pub sold_token: String,
    pub remaining: String,
    pub entry_quote: String,
    pub exit_quote: String,
    pub realized_pnl: String,
    pub inventory_ok: bool,
    pub pnl_ok: bool,
    pub negative_inventory: bool,
    pub oversold: bool,
    pub closed_without_exit_fill: bool,
    pub failed_exit_marked_closed: bool,
}

impl ReconciledPosition {
    pub fn ok(&self) -> bool {
        self.inventory_ok
            && self.pnl_ok
            && !self.negative_inventory
            && !self.oversold
            && !self.closed_without_exit_fill
            && !self.failed_exit_marked_closed
    }
}

/// Proportional cost of sold tokens: `entry_quote * sold / entry_token`.
/// Fees are assumed already netted in fill quotes (do not subtract again).
pub fn realized_pnl_from_fills(
    entry_token: &U256,
    entry_quote: &U256,
    sold: &U256,
    exit_quote: &U256,
) -> String {
    if entry_token.is_zero() {
        return "0".into();
    }
    let sold_cost = entry_quote.saturating_mul(*sold) / *entry_token;
    if *exit_quote >= sold_cost {
        format!("+{}", u256_dec(*exit_quote - sold_cost))
    } else {
        format!("-{}", u256_dec(sold_cost - *exit_quote))
    }
}

fn signed_eq(a: &str, b: &str) -> bool {
    parse_signed(a) == parse_signed(b)
}

fn parse_signed(s: &str) -> (bool, U256) {
    let t = s.trim();
    if let Some(x) = t.strip_prefix('-') {
        (true, parse_u256(x))
    } else if let Some(x) = t.strip_prefix('+') {
        (false, parse_u256(x))
    } else {
        (false, parse_u256(t))
    }
}

pub fn reconcile_position(
    pos: &SimulatedPosition,
    buy_fills: &[FillLeg],
    sell_fills: &[FillLeg],
) -> ReconciledPosition {
    let mut entry_token = U256::ZERO;
    let mut entry_quote = U256::ZERO;
    for f in buy_fills {
        entry_token = entry_token.saturating_add(parse_u256(&f.token));
        entry_quote = entry_quote.saturating_add(parse_u256(&f.quote));
    }
    if entry_token.is_zero() {
        entry_token = parse_u256(&pos.initial_token_amount);
        entry_quote = parse_u256(&pos.quote_cost);
    }
    let mut sold = U256::ZERO;
    let mut exit_quote = U256::ZERO;
    for f in sell_fills {
        sold = sold.saturating_add(parse_u256(&f.token));
        exit_quote = exit_quote.saturating_add(parse_u256(&f.quote));
    }
    let oversold = sold > entry_token;
    let remaining = if oversold {
        U256::ZERO
    } else {
        entry_token - sold
    };
    let pnl = realized_pnl_from_fills(
        &entry_token,
        &entry_quote,
        &sold.min(entry_token),
        &exit_quote,
    );
    let stored_rem = parse_u256(&pos.remaining_token_amount);
    let closed = pos.status == PositionStatus::Closed;
    let has_sell = !sell_fills.is_empty();
    ReconciledPosition {
        position_id: pos.id,
        token: pos.token.clone(),
        strategy_policy_id: pos.strategy_policy_id.clone(),
        entry_token: u256_dec(entry_token),
        sold_token: u256_dec(sold),
        remaining: u256_dec(remaining),
        entry_quote: u256_dec(entry_quote),
        exit_quote: u256_dec(exit_quote),
        realized_pnl: pnl.clone(),
        inventory_ok: stored_rem == remaining,
        pnl_ok: signed_eq(&pos.realized_pnl_quote, &pnl),
        negative_inventory: false,
        oversold,
        closed_without_exit_fill: closed && !has_sell,
        failed_exit_marked_closed: closed && remaining > U256::ZERO,
    }
}
