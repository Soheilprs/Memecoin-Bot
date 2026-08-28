use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::Value;

use crate::domain::raw_event::{
    CanonicalStatus, DecoderStatus, EvmLog, Finality, RawEvent, RawEventKind, SolanaInstruction,
};
use crate::domain::Chain;

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

pub fn fixture_path(rel: &str) -> PathBuf {
    workspace_root().join("tests/fixtures").join(rel)
}

pub fn load_json(rel: &str) -> Value {
    let path = fixture_path(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&text).expect("fixture json")
}

pub fn evm_raw_from_fixture(rel: &str) -> RawEvent {
    let v = load_json(rel);
    evm_raw_from_value(&v)
}

pub fn evm_raw_from_value(v: &Value) -> RawEvent {
    let chain = match v["chain"]
        .as_str()
        .or_else(|| v["provenance"]["chain"].as_str())
    {
        Some("base") => Chain::Base,
        Some("robinhood") => Chain::Robinhood,
        other => panic!("unexpected chain {other:?}"),
    };
    let chain_id = v["chain_id"]
        .as_u64()
        .or_else(|| v["provenance"]["chain_id"].as_u64())
        .unwrap_or_else(|| chain.evm_chain_id().unwrap());
    let ts = v
        .get("block_timestamp")
        .and_then(Value::as_i64)
        .map(|t| Utc.timestamp_opt(t, 0).single().unwrap());
    let log = EvmLog {
        chain,
        chain_id,
        address: v["address"].as_str().unwrap().to_string(),
        topics: v["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect(),
        data: v["data"].as_str().unwrap().to_string(),
        block_number: v["block_number"].as_u64(),
        block_hash: v["block_hash"].as_str().map(|s| s.to_string()),
        transaction_hash: v["transaction_hash"].as_str().unwrap().to_string(),
        transaction_index: v["transaction_index"].as_u64(),
        log_index: v["log_index"].as_u64().unwrap(),
        removed: v["removed"].as_bool().unwrap_or(false),
        block_timestamp: ts,
        tx_from: v["tx_from"].as_str().map(|s| s.to_string()),
    };
    RawEvent {
        kind: RawEventKind::Evm(log),
        source: "fixture".into(),
        observed_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        decoder_status: DecoderStatus::Pending,
        decoder_version: None,
        error: None,
    }
}

pub fn pumpfun_raw_from_fixture() -> RawEvent {
    solana_raw_from_fixture("solana/pumpfun/create_v2.json", "create_instruction_index")
}

pub fn solana_raw_from_fixture(rel: &str, index_field: &str) -> RawEvent {
    let v = load_json(rel);
    if v.get("transaction").is_some() {
        return solana_raw_from_get_transaction(&v, index_field);
    }
    let keys: Vec<String> = v["account_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    let ix_index = v[index_field]
        .as_u64()
        .or_else(|| v["instruction_index"].as_u64())
        .unwrap() as usize;
    let ix = &v["instructions"][ix_index];
    let program_id = keys[ix["programIdIndex"].as_u64().unwrap() as usize].clone();
    let accounts = ix["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|idx| keys[idx.as_u64().unwrap() as usize].clone())
        .collect();
    let logs = v["log_messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect();
    let block_time = v["block_time"]
        .as_i64()
        .map(|t| Utc.timestamp_opt(t, 0).single().unwrap());
    let instruction = SolanaInstruction {
        program_id,
        accounts,
        data_base58: ix["data"].as_str().unwrap().to_string(),
        signature: v["signature"].as_str().unwrap().to_string(),
        slot: v["slot"].as_u64(),
        block_time,
        transaction_index: None,
        instruction_index: ix_index as u32,
        inner_instruction_index: v["inner_instruction_index"].as_u64().map(|v| v as u32),
        log_messages: logs,
        account_keys: keys,
        inner_instructions: Vec::new(),
        finality: Finality::Confirmed,
        execution_status: crate::domain::ExecutionStatus::Success,
        token_balances: serde_json::json!({}),
    };
    RawEvent {
        kind: RawEventKind::Solana(instruction),
        source: "fixture".into(),
        observed_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        decoder_status: DecoderStatus::Pending,
        decoder_version: None,
        error: None,
    }
}

pub fn solana_raw_from_get_transaction(v: &Value, index_field: &str) -> RawEvent {
    let events = crate::ingest::solana::parse::raw_events_from_get_transaction(
        v,
        "fixture",
        Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        Finality::Confirmed,
    );
    let inner = v.get("inner_instruction_index").and_then(Value::as_u64);
    let ix = v
        .get(index_field)
        .and_then(Value::as_u64)
        .or_else(|| v.get("trade_instruction_index").and_then(Value::as_u64))
        .or_else(|| v.get("migrate_instruction_index").and_then(Value::as_u64));
    if let Some(want_ix) = ix {
        if let Some(raw) = events.iter().find(|e| {
            e.instruction_index() == Some(want_ix as i32)
                && e.inner_instruction_index() == inner.map(|x| x as i32)
        }) {
            return raw.clone();
        }
        if let Some(raw) = events
            .iter()
            .find(|e| e.instruction_index() == Some(want_ix as i32))
        {
            return raw.clone();
        }
    }
    events
        .into_iter()
        .next()
        .expect("solana fixture produced no pump instruction")
}

pub fn unknown_evm(chain: Chain, factory: &str) -> RawEvent {
    let chain_id = chain.evm_chain_id().unwrap();
    RawEvent {
        kind: RawEventKind::Evm(EvmLog {
            chain,
            chain_id,
            address: factory.to_string(),
            topics: vec![
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ],
            data: "0x".into(),
            block_number: Some(1),
            block_hash: Some(
                "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
            ),
            transaction_hash: "0x2222222222222222222222222222222222222222222222222222222222222222"
                .into(),
            transaction_index: Some(0),
            log_index: 0,
            removed: false,
            block_timestamp: None,
            tx_from: None,
        }),
        source: "test".into(),
        observed_at: Utc::now(),
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Unknown,
        decoder_status: DecoderStatus::Pending,
        decoder_version: None,
        error: None,
    }
}

pub fn _path_exists(p: &Path) -> bool {
    p.exists()
}
