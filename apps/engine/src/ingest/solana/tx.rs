use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::decoders::pumpfun::classify_pump_ix;
use crate::decoders::pumpswap::classify_pumpswap_ix;
use crate::decoders::solana_buf::decode_ix_data;
use crate::domain::raw_event::{
    CanonicalStatus, DecoderStatus, ExecutionStatus, Finality, RawEvent, RawEventKind,
    SolanaCompiledIx, SolanaInnerInstructions, SolanaInstruction,
};
use crate::registry::{PUMPFUN_PROGRAM, PUMPSWAP_PROGRAM};

/// Transport-neutral Solana transaction view used by JSON-RPC, Yellowstone,
/// and offline fixtures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaTxView {
    pub signature: String,
    pub slot: Option<u64>,
    pub block_time: Option<DateTime<Utc>>,
    pub transaction_index: Option<u32>,
    pub account_keys: Vec<String>,
    pub instructions: Vec<SolanaCompiledIx>,
    pub inner_instructions: Vec<SolanaInnerInstructions>,
    pub log_messages: Vec<String>,
    pub err: Option<serde_json::Value>,
    pub token_balances: serde_json::Value,
    pub finality: Finality,
    pub source: String,
    pub observed_at: DateTime<Utc>,
    /// Original stream payload for replay (JSON-RPC result or proto JSON).
    pub raw_payload: serde_json::Value,
}

impl SolanaTxView {
    pub fn failed(&self) -> bool {
        self.err.as_ref().is_some_and(|e| !e.is_null())
    }

    pub fn execution_status(&self) -> ExecutionStatus {
        if self.failed() {
            ExecutionStatus::Failed
        } else {
            ExecutionStatus::Success
        }
    }
}

pub fn raw_events_from_view(view: &SolanaTxView) -> Vec<RawEvent> {
    let mut events = Vec::new();
    let status = view.execution_status();
    let inner = view.inner_instructions.clone();
    for (i, ix) in view.instructions.iter().enumerate() {
        if let Some(raw) = ix_to_raw(view, ix, i as u32, None, status, inner.clone()) {
            events.push(raw);
        }
    }
    for group in &view.inner_instructions {
        for (j, ix) in group.instructions.iter().enumerate() {
            if let Some(raw) =
                ix_to_raw(view, ix, group.index, Some(j as u32), status, inner.clone())
            {
                events.push(raw);
            }
        }
    }
    events
}

fn ix_to_raw(
    view: &SolanaTxView,
    ix: &SolanaCompiledIx,
    instruction_index: u32,
    inner: Option<u32>,
    status: ExecutionStatus,
    inner_instructions: Vec<SolanaInnerInstructions>,
) -> Option<RawEvent> {
    let tracked = ix.program_id == PUMPFUN_PROGRAM || ix.program_id == PUMPSWAP_PROGRAM;
    if !tracked {
        return None;
    }
    let data = decode_ix_data(&ix.data_base58).ok()?;
    let interesting = if ix.program_id == PUMPFUN_PROGRAM {
        classify_pump_ix(&data).is_some()
    } else {
        classify_pumpswap_ix(&data).is_some()
    };
    if !interesting {
        return None;
    }
    Some(RawEvent {
        kind: RawEventKind::Solana(SolanaInstruction {
            program_id: ix.program_id.clone(),
            accounts: ix.accounts.clone(),
            data_base58: ix.data_base58.clone(),
            signature: view.signature.clone(),
            slot: view.slot,
            block_time: view.block_time,
            transaction_index: view.transaction_index,
            instruction_index,
            inner_instruction_index: inner,
            log_messages: view.log_messages.clone(),
            account_keys: view.account_keys.clone(),
            inner_instructions,
            finality: view.finality,
            execution_status: status,
            token_balances: view.token_balances.clone(),
        }),
        source: view.source.clone(),
        observed_at: view.observed_at,
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: view.finality,
        decoder_status: DecoderStatus::Pending,
        decoder_version: None,
        error: None,
    })
}

pub fn view_from_get_transaction(
    tx: &serde_json::Value,
    source: &str,
    observed_at: DateTime<Utc>,
    finality: Finality,
) -> Option<SolanaTxView> {
    let result = tx.get("result").unwrap_or(tx);
    if result.is_null() {
        return None;
    }
    let slot = result.get("slot").and_then(serde_json::Value::as_u64);
    let block_time = result
        .get("blockTime")
        .and_then(serde_json::Value::as_i64)
        .and_then(|t| Utc.timestamp_opt(t, 0).single());
    let sig = result
        .pointer("/transaction/signatures/0")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let message = result
        .pointer("/transaction/message")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let mut keys = account_keys(&message);
    if let Some(loaded) = result.pointer("/meta/loadedAddresses") {
        if let Some(w) = loaded.get("writable").and_then(serde_json::Value::as_array) {
            keys.extend(w.iter().filter_map(|v| v.as_str().map(|s| s.to_string())));
        }
        if let Some(r) = loaded.get("readonly").and_then(serde_json::Value::as_array) {
            keys.extend(r.iter().filter_map(|v| v.as_str().map(|s| s.to_string())));
        }
    }
    let logs: Vec<String> = result
        .pointer("/meta/logMessages")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let inner = parse_inner(result.pointer("/meta/innerInstructions"), &keys);
    let instructions = message
        .get("instructions")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(|ix| compiled(ix, &keys)).collect())
        .unwrap_or_default();
    let err = result.pointer("/meta/err").cloned();
    let token_balances = serde_json::json!({
        "pre": result.pointer("/meta/preTokenBalances").cloned().unwrap_or(serde_json::Value::Null),
        "post": result.pointer("/meta/postTokenBalances").cloned().unwrap_or(serde_json::Value::Null),
    });
    Some(SolanaTxView {
        signature: sig,
        slot,
        block_time,
        transaction_index: None,
        account_keys: keys,
        instructions,
        inner_instructions: inner,
        log_messages: logs,
        err: if err.as_ref().is_some_and(|e| e.is_null()) {
            None
        } else {
            err
        },
        token_balances,
        finality,
        source: source.into(),
        observed_at,
        raw_payload: result.clone(),
    })
}

fn account_keys(message: &serde_json::Value) -> Vec<String> {
    let Some(arr) = message
        .get("accountKeys")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                v.get("pubkey")
                    .and_then(serde_json::Value::as_str)
                    .map(|s| s.to_string())
            }
        })
        .collect()
}

fn parse_inner(v: Option<&serde_json::Value>, keys: &[String]) -> Vec<SolanaInnerInstructions> {
    let Some(arr) = v.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|g| {
            let index = g.get("index").and_then(serde_json::Value::as_u64)? as u32;
            let ixs = g
                .get("instructions")
                .and_then(serde_json::Value::as_array)?;
            let instructions = ixs.iter().filter_map(|ix| compiled(ix, keys)).collect();
            Some(SolanaInnerInstructions {
                index,
                instructions,
            })
        })
        .collect()
}

fn compiled(ix: &serde_json::Value, keys: &[String]) -> Option<SolanaCompiledIx> {
    let program_id = if let Some(idx) = ix.get("programIdIndex").and_then(serde_json::Value::as_u64)
    {
        keys.get(idx as usize)?.clone()
    } else {
        ix.get("programId")?.as_str()?.to_string()
    };
    let accounts = ix
        .get("accounts")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    if let Some(i) = v.as_u64() {
                        keys.get(i as usize).cloned()
                    } else {
                        v.as_str().map(|s| s.to_string())
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let data_base58 = ix.get("data")?.as_str()?.to_string();
    Some(SolanaCompiledIx {
        program_id,
        accounts,
        data_base58,
    })
}
