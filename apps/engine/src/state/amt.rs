//! Exact integer amounts as decimal strings. Never f64.

use alloy_primitives::U256;
use std::str::FromStr;

pub fn parse_u256(s: &str) -> U256 {
    let t = s.trim();
    if t.is_empty() {
        return U256::ZERO;
    }
    U256::from_str(t).unwrap_or(U256::ZERO)
}

pub fn u256_dec(v: U256) -> String {
    format!("{v}")
}

pub fn add_raw(a: &str, b: &str) -> String {
    u256_dec(parse_u256(a).saturating_add(parse_u256(b)))
}

pub fn sub_sat_raw(a: &str, b: &str) -> String {
    u256_dec(parse_u256(a).saturating_sub(parse_u256(b)))
}

pub fn net_signed(buy: &str, sell: &str) -> String {
    let b = parse_u256(buy);
    let s = parse_u256(sell);
    if b >= s {
        u256_dec(b - s)
    } else {
        format!("-{}", u256_dec(s - b))
    }
}

pub fn ratio_bps(numer: &str, denom: &str) -> Option<u32> {
    let d = parse_u256(denom);
    if d.is_zero() {
        return None;
    }
    let n = parse_u256(numer);
    let bps = n.saturating_mul(U256::from(10_000u64)) / d;
    let v = if bps > U256::from(10_000u64) {
        10_000
    } else {
        u32::try_from(bps).unwrap_or(10_000)
    };
    Some(v)
}

pub fn min_raw(a: &str, b: &str) -> String {
    if parse_u256(a) <= parse_u256(b) {
        a.to_string()
    } else {
        b.to_string()
    }
}

pub fn max_raw(a: &str, b: &str) -> String {
    if parse_u256(a) >= parse_u256(b) {
        a.to_string()
    } else {
        b.to_string()
    }
}

pub fn median_raw(values: &mut [U256]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    values.sort();
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(u256_dec(values[mid]))
    } else {
        let a = values[mid - 1];
        let b = values[mid];
        Some(u256_dec(a + (b - a) / U256::from(2u64)))
    }
}
