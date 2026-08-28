//! Convert Yellowstone gRPC transaction updates into SolanaTxView.

use chrono::{TimeZone, Utc};

use crate::domain::raw_event::{Finality, SolanaCompiledIx, SolanaInnerInstructions};
use crate::ingest::solana::tx::SolanaTxView;

use yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof;
use yellowstone_grpc_proto::geyser::{SubscribeUpdate, SubscribeUpdateTransactionInfo};
use yellowstone_grpc_proto::solana::storage::confirmed_block::{
    CompiledInstruction, InnerInstructions, TransactionStatusMeta,
};

pub fn view_from_subscribe_update(
    update: &SubscribeUpdate,
    source: &str,
    observed_at: chrono::DateTime<Utc>,
    finality: Finality,
) -> Option<SolanaTxView> {
    match update.update_oneof.as_ref()? {
        UpdateOneof::Transaction(tx) => {
            let info = tx.transaction.as_ref()?;
            view_from_tx_info(info, tx.slot, source, observed_at, finality)
        }
        _ => None,
    }
}

pub fn view_from_tx_info(
    info: &SubscribeUpdateTransactionInfo,
    slot: u64,
    source: &str,
    observed_at: chrono::DateTime<Utc>,
    finality: Finality,
) -> Option<SolanaTxView> {
    let signature = if info.signature.is_empty() {
        return None;
    } else {
        bs58::encode(&info.signature).into_string()
    };
    let tx = info.transaction.as_ref()?;
    let message = tx.message.as_ref()?;
    let mut keys: Vec<String> = message
        .account_keys
        .iter()
        .map(|k| bs58::encode(k).into_string())
        .collect();
    let meta: Option<&TransactionStatusMeta> = info.meta.as_ref();
    if let Some(m) = meta {
        keys.extend(
            m.loaded_writable_addresses
                .iter()
                .map(|k| bs58::encode(k).into_string()),
        );
        keys.extend(
            m.loaded_readonly_addresses
                .iter()
                .map(|k| bs58::encode(k).into_string()),
        );
    }
    let instructions = message
        .instructions
        .iter()
        .filter_map(|ix| compiled_from_proto(ix, &keys))
        .collect();
    let inner = meta
        .map(|m| inner_from_proto(&m.inner_instructions, &keys))
        .unwrap_or_default();
    let logs = meta.map(|m| m.log_messages.clone()).unwrap_or_default();
    let err = meta.and_then(|m| {
        m.err
            .as_ref()
            .map(|e| serde_json::json!({"err": format!("{e:?}")}))
    });
    let token_balances = meta
        .map(|m| {
            serde_json::json!({
                "pre": format!("{:?}", m.pre_token_balances),
                "post": format!("{:?}", m.post_token_balances),
            })
        })
        .unwrap_or_else(|| serde_json::json!({}));
    let raw_payload = serde_json::json!({
        "signature": signature,
        "slot": slot,
        "index": info.index,
        "is_vote": info.is_vote,
        "account_keys": keys,
        "log_messages": logs,
        "failed": err.is_some(),
    });
    Some(SolanaTxView {
        signature,
        slot: Some(slot),
        block_time: None,
        transaction_index: Some(info.index as u32),
        account_keys: keys,
        instructions,
        inner_instructions: inner,
        log_messages: logs,
        err,
        token_balances,
        finality,
        source: source.into(),
        observed_at,
        raw_payload,
    })
}

fn compiled_from_proto(ix: &CompiledInstruction, keys: &[String]) -> Option<SolanaCompiledIx> {
    let program_id = keys.get(ix.program_id_index as usize)?.clone();
    let accounts = ix
        .accounts
        .iter()
        .filter_map(|idx| keys.get(*idx as usize).cloned())
        .collect();
    Some(SolanaCompiledIx {
        program_id,
        accounts,
        data_base58: bs58::encode(&ix.data).into_string(),
    })
}

fn inner_from_proto(groups: &[InnerInstructions], keys: &[String]) -> Vec<SolanaInnerInstructions> {
    groups
        .iter()
        .map(|g| SolanaInnerInstructions {
            index: g.index,
            instructions: g
                .instructions
                .iter()
                .filter_map(|inner| {
                    compiled_from_proto(
                        &CompiledInstruction {
                            program_id_index: inner.program_id_index,
                            accounts: inner.accounts.clone(),
                            data: inner.data.clone(),
                        },
                        keys,
                    )
                })
                .collect(),
        })
        .collect()
}

pub fn _keep_utc(ts: i64) -> Option<chrono::DateTime<Utc>> {
    Utc.timestamp_opt(ts, 0).single()
}

/// Build a Yellowstone `SubscribeUpdate` from a captured JSON-RPC `getTransaction` body.
/// Used for offline protobuf → RawEvent tests with real fixtures.
pub fn subscribe_update_from_rpc_json(
    tx: &serde_json::Value,
    transaction_index: u64,
) -> Option<SubscribeUpdate> {
    use yellowstone_grpc_proto::geyser::SubscribeUpdateTransaction;
    use yellowstone_grpc_proto::solana::storage::confirmed_block::{
        InnerInstructions, Message, Transaction, TransactionError, TransactionStatusMeta,
    };

    let result = tx.get("result").unwrap_or(tx);
    let slot = result.get("slot").and_then(serde_json::Value::as_u64)?;
    let sig_str = result
        .pointer("/transaction/signatures/0")
        .and_then(serde_json::Value::as_str)?;
    let signature = bs58::decode(sig_str).into_vec().ok()?;
    let message_json = result.pointer("/transaction/message")?;
    let account_keys: Vec<Vec<u8>> = message_json
        .get("accountKeys")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .filter_map(|v| {
            let s = v
                .as_str()
                .or_else(|| v.get("pubkey").and_then(serde_json::Value::as_str))?;
            bs58::decode(s).into_vec().ok()
        })
        .collect();
    let instructions = message_json
        .get("instructions")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(compiled_proto_from_json).collect())
        .unwrap_or_default();
    let loaded_w = decode_pk_array(result.pointer("/meta/loadedAddresses/writable"));
    let loaded_r = decode_pk_array(result.pointer("/meta/loadedAddresses/readonly"));
    let logs = result
        .pointer("/meta/logMessages")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let inner = result
        .pointer("/meta/innerInstructions")
        .and_then(serde_json::Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|g| {
                    Some(InnerInstructions {
                        index: g.get("index").and_then(serde_json::Value::as_u64)? as u32,
                        instructions: g
                            .get("instructions")
                            .and_then(serde_json::Value::as_array)?
                            .iter()
                            .filter_map(inner_proto_from_json)
                            .collect(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let failed = result
        .pointer("/meta/err")
        .is_some_and(|e| !e.is_null() && e != &serde_json::Value::Bool(false));
    let info = SubscribeUpdateTransactionInfo {
        signature: signature.clone(),
        is_vote: false,
        transaction: Some(Transaction {
            signatures: vec![signature],
            message: Some(Message {
                account_keys,
                instructions,
                ..Default::default()
            }),
        }),
        meta: Some(TransactionStatusMeta {
            err: failed.then_some(TransactionError {
                err: b"failed".to_vec(),
            }),
            log_messages: logs,
            inner_instructions: inner,
            loaded_writable_addresses: loaded_w,
            loaded_readonly_addresses: loaded_r,
            ..Default::default()
        }),
        index: transaction_index,
    };
    Some(SubscribeUpdate {
        filters: vec!["pumpfun".into()],
        update_oneof: Some(UpdateOneof::Transaction(SubscribeUpdateTransaction {
            transaction: Some(info),
            slot,
        })),
        created_at: None,
    })
}

fn decode_pk_array(v: Option<&serde_json::Value>) -> Vec<Vec<u8>> {
    v.and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| bs58::decode(s).into_vec().ok()))
                .collect()
        })
        .unwrap_or_default()
}

fn compiled_proto_from_json(
    ix: &serde_json::Value,
) -> Option<yellowstone_grpc_proto::solana::storage::confirmed_block::CompiledInstruction> {
    let program_id_index = ix
        .get("programIdIndex")
        .and_then(serde_json::Value::as_u64)? as u32;
    let accounts = ix
        .get("accounts")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_u64().map(|i| i as u8))
                .collect()
        })
        .unwrap_or_default();
    let data = bs58::decode(ix.get("data")?.as_str()?).into_vec().ok()?;
    Some(
        yellowstone_grpc_proto::solana::storage::confirmed_block::CompiledInstruction {
            program_id_index,
            accounts,
            data,
        },
    )
}

fn inner_proto_from_json(
    ix: &serde_json::Value,
) -> Option<yellowstone_grpc_proto::solana::storage::confirmed_block::InnerInstruction> {
    let compiled = compiled_proto_from_json(ix)?;
    Some(
        yellowstone_grpc_proto::solana::storage::confirmed_block::InnerInstruction {
            program_id_index: compiled.program_id_index,
            accounts: compiled.accounts,
            data: compiled.data,
            stack_height: ix
                .get("stackHeight")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoders::pumpfun::CREATE_V2_DISCRIMINATOR;
    use crate::ingest::solana::tx::raw_events_from_view;
    use crate::registry::PUMPFUN_PROGRAM;
    use yellowstone_grpc_proto::solana::storage::confirmed_block::{
        CompiledInstruction, Message, Transaction, TransactionStatusMeta,
    };

    #[test]
    fn empty_tx_info_is_none() {
        let info = SubscribeUpdateTransactionInfo::default();
        assert!(view_from_tx_info(&info, 1, "t", Utc::now(), Finality::Processed).is_none());
    }

    #[test]
    fn proto_create_v2_becomes_rawevent() {
        let pk = bs58::decode(PUMPFUN_PROGRAM).into_vec().unwrap();
        let mut data = CREATE_V2_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&[0u8; 8]);
        let info = SubscribeUpdateTransactionInfo {
            signature: vec![7u8; 64],
            is_vote: false,
            transaction: Some(Transaction {
                signatures: vec![vec![7u8; 64]],
                message: Some(Message {
                    account_keys: vec![pk],
                    instructions: vec![CompiledInstruction {
                        program_id_index: 0,
                        accounts: vec![0],
                        data,
                    }],
                    ..Default::default()
                }),
            }),
            meta: Some(TransactionStatusMeta {
                log_messages: vec!["Program log: Instruction: CreateV2".into()],
                ..Default::default()
            }),
            index: 9,
        };
        let view = view_from_tx_info(&info, 42, "yellowstone", Utc::now(), Finality::Processed)
            .expect("view");
        assert_eq!(view.slot, Some(42));
        assert_eq!(view.transaction_index, Some(9));
        let events = raw_events_from_view(&view);
        assert!(!events.is_empty());
        assert_eq!(events[0].as_solana().unwrap().transaction_index, Some(9));
    }
}
