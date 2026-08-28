use alloy_primitives::{Address, Bytes, B256, U256};

use crate::domain::raw_event::normalize_address;
use crate::error::{EngineError, Result};

pub fn parse_address(value: &str) -> Result<Address> {
    normalize_address(value)
        .parse()
        .map_err(|e| EngineError::Malformed(format!("address {value}: {e}")))
}

pub fn parse_b256(value: &str) -> Result<B256> {
    normalize_address(value)
        .parse()
        .map_err(|e| EngineError::Malformed(format!("b256 {value}: {e}")))
}

pub fn parse_bytes(value: &str) -> Result<Bytes> {
    let hex = value
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let bytes = hex::decode(hex).map_err(|e| EngineError::Malformed(format!("data hex: {e}")))?;
    Ok(Bytes::from(bytes))
}

pub fn u256_dec(v: U256) -> String {
    v.to_string()
}

pub fn i128_abs_dec(v: i128) -> String {
    v.unsigned_abs().to_string()
}

pub fn topic_matches(topic: &str, expected: &str) -> bool {
    normalize_address(topic) == normalize_address(expected)
}
