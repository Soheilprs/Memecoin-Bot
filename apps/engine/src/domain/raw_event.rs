use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::chain::Chain;
use super::corpus::CorpusRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finality {
    Processed,
    Confirmed,
    Finalized,
    Unknown,
}

impl Finality {
    pub fn as_str(self) -> &'static str {
        match self {
            Finality::Processed => "processed",
            Finality::Confirmed => "confirmed",
            Finality::Finalized => "finalized",
            Finality::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalStatus {
    Canonical,
    Orphaned,
}

impl CanonicalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CanonicalStatus::Canonical => "canonical",
            CanonicalStatus::Orphaned => "orphaned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderStatus {
    Pending,
    Success,
    Unknown,
    Error,
}

impl DecoderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DecoderStatus::Pending => "pending",
            DecoderStatus::Success => "success",
            DecoderStatus::Unknown => "unknown",
            DecoderStatus::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmLog {
    pub chain: Chain,
    pub chain_id: u64,
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub transaction_hash: String,
    pub transaction_index: Option<u64>,
    pub log_index: u64,
    pub removed: bool,
    pub block_timestamp: Option<DateTime<Utc>>,
    pub tx_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaInstruction {
    pub program_id: String,
    pub accounts: Vec<String>,
    pub data_base58: String,
    pub signature: String,
    pub slot: Option<u64>,
    pub block_time: Option<DateTime<Utc>>,
    pub transaction_index: Option<u32>,
    pub instruction_index: u32,
    pub inner_instruction_index: Option<u32>,
    pub log_messages: Vec<String>,
    pub account_keys: Vec<String>,
    pub inner_instructions: Vec<SolanaInnerInstructions>,
    pub finality: Finality,
    pub execution_status: ExecutionStatus,
    pub token_balances: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failed,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionStatus::Success => "success",
            ExecutionStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaInnerInstructions {
    pub index: u32,
    pub instructions: Vec<SolanaCompiledIx>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaCompiledIx {
    pub program_id: String,
    pub accounts: Vec<String>,
    pub data_base58: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawEventKind {
    Evm(EvmLog),
    Solana(SolanaInstruction),
    /// Decoded research table row. Not a raw Solana instruction.
    DecodedCorpus(Box<CorpusRecord>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvent {
    pub kind: RawEventKind,
    pub source: String,
    pub observed_at: DateTime<Utc>,
    pub persisted_at: Option<DateTime<Utc>>,
    pub canonical_status: CanonicalStatus,
    pub finality: Finality,
    pub decoder_status: DecoderStatus,
    pub decoder_version: Option<String>,
    pub error: Option<String>,
}

impl RawEvent {
    pub fn chain(&self) -> Chain {
        match &self.kind {
            RawEventKind::Evm(log) => log.chain,
            RawEventKind::Solana(_) | RawEventKind::DecodedCorpus(_) => Chain::Solana,
        }
    }

    pub fn tx_hash(&self) -> &str {
        match &self.kind {
            RawEventKind::Evm(log) => &log.transaction_hash,
            RawEventKind::Solana(ix) => &ix.signature,
            RawEventKind::DecodedCorpus(c) => c.signature.as_deref().unwrap_or(c.mint.as_str()),
        }
    }

    pub fn log_index(&self) -> Option<u64> {
        match &self.kind {
            RawEventKind::Evm(log) => Some(log.log_index),
            RawEventKind::Solana(_) => None,
            RawEventKind::DecodedCorpus(c) => Some(c.source_row),
        }
    }

    pub fn transaction_index(&self) -> Option<i64> {
        match &self.kind {
            RawEventKind::Evm(log) => log.transaction_index.map(|v| v as i64),
            RawEventKind::Solana(ix) => ix.transaction_index.map(|v| v as i64),
            RawEventKind::DecodedCorpus(c) => c
                .transaction_index
                .map(|v| v as i64)
                .or(Some(c.order_seq as i64)),
        }
    }

    pub fn instruction_index(&self) -> Option<i32> {
        match &self.kind {
            RawEventKind::Evm(_) => None,
            RawEventKind::Solana(ix) => Some(ix.instruction_index as i32),
            RawEventKind::DecodedCorpus(c) => c.instruction_index.map(|v| v as i32),
        }
    }

    pub fn inner_instruction_index(&self) -> Option<i32> {
        match &self.kind {
            RawEventKind::Evm(_) => None,
            RawEventKind::Solana(ix) => ix.inner_instruction_index.map(|v| v as i32),
            RawEventKind::DecodedCorpus(c) => c.inner_instruction_index.map(|v| v as i32),
        }
    }

    pub fn block_number(&self) -> Option<i64> {
        match &self.kind {
            RawEventKind::Evm(log) => log.block_number.map(|v| v as i64),
            RawEventKind::Solana(_) | RawEventKind::DecodedCorpus(_) => None,
        }
    }

    pub fn block_hash(&self) -> Option<&str> {
        match &self.kind {
            RawEventKind::Evm(log) => log.block_hash.as_deref(),
            RawEventKind::Solana(_) | RawEventKind::DecodedCorpus(_) => None,
        }
    }

    pub fn slot(&self) -> Option<i64> {
        match &self.kind {
            RawEventKind::Evm(_) => None,
            RawEventKind::Solana(ix) => ix.slot.map(|v| v as i64),
            RawEventKind::DecodedCorpus(c) => c.slot.map(|v| v as i64),
        }
    }

    pub fn chain_time(&self) -> Option<DateTime<Utc>> {
        match &self.kind {
            RawEventKind::Evm(log) => log.block_timestamp,
            RawEventKind::Solana(ix) => ix.block_time,
            RawEventKind::DecodedCorpus(c) => Some(c.timestamp),
        }
    }

    pub fn identity_string(&self) -> String {
        match &self.kind {
            RawEventKind::Evm(log) => format!(
                "evm|{}|{}|{}",
                log.chain_id,
                normalize_hex(&log.transaction_hash),
                log.log_index
            ),
            RawEventKind::Solana(ix) => {
                let inner = ix
                    .inner_instruction_index
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string());
                format!("solana|{}|{}|{}", ix.signature, ix.instruction_index, inner)
            }
            RawEventKind::DecodedCorpus(c) => c.identity_string(),
        }
    }

    pub fn event_id(&self) -> String {
        let digest = Sha256::digest(self.identity_string().as_bytes());
        hex::encode(digest)
    }

    pub fn as_evm(&self) -> Option<&EvmLog> {
        match &self.kind {
            RawEventKind::Evm(log) => Some(log),
            RawEventKind::Solana(_) | RawEventKind::DecodedCorpus(_) => None,
        }
    }

    pub fn as_solana(&self) -> Option<&SolanaInstruction> {
        match &self.kind {
            RawEventKind::Solana(ix) => Some(ix),
            RawEventKind::Evm(_) | RawEventKind::DecodedCorpus(_) => None,
        }
    }

    pub fn as_corpus(&self) -> Option<&CorpusRecord> {
        match &self.kind {
            RawEventKind::DecodedCorpus(c) => Some(c),
            RawEventKind::Evm(_) | RawEventKind::Solana(_) => None,
        }
    }
}

pub fn normalize_hex(value: &str) -> String {
    let v = value.trim();
    let v = v
        .strip_prefix("0x")
        .or_else(|| v.strip_prefix("0X"))
        .unwrap_or(v);
    format!("0x{}", v.to_ascii_lowercase())
}

pub fn normalize_address(value: &str) -> String {
    normalize_hex(value)
}
