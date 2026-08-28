use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::domain::RawEvent;
use crate::error::Result;
use crate::storage::Checkpoint;

pub mod backoff;
pub mod evm;
pub mod rpc_json;
pub mod solana;

#[async_trait]
pub trait ChainIngest: Send + Sync {
    async fn run(&self, sender: Sender<RawEvent>) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct ResumePlan {
    pub from_block: Option<u64>,
    pub from_slot: Option<u64>,
    pub overlap_blocks: u64,
    pub overlap_slots: u64,
}

impl ResumePlan {
    pub fn for_evm(checkpoint: Option<&Checkpoint>, head: u64) -> Self {
        let overlap = checkpoint
            .map(|c| c.overlap_blocks.max(1) as u64)
            .unwrap_or(64);
        let from_block = checkpoint
            .and_then(|c| c.last_block)
            .map(|b| (b as u64).saturating_sub(overlap));
        let _ = head;
        Self {
            from_block,
            from_slot: None,
            overlap_blocks: overlap,
            overlap_slots: 0,
        }
    }

    pub fn for_solana(checkpoint: Option<&Checkpoint>, head: u64) -> Self {
        let overlap = checkpoint
            .map(|c| c.overlap_slots.max(1) as u64)
            .unwrap_or(32);
        let from_slot = checkpoint
            .and_then(|c| c.last_slot)
            .map(|s| (s as u64).saturating_sub(overlap));
        let _ = head;
        Self {
            from_block: None,
            from_slot,
            overlap_blocks: 0,
            overlap_slots: overlap,
        }
    }
}

pub struct ReplayIngest {
    pub events: Vec<RawEvent>,
}

#[async_trait]
impl ChainIngest for ReplayIngest {
    async fn run(&self, sender: Sender<RawEvent>) -> Result<()> {
        for event in &self.events {
            sender
                .send(event.clone())
                .await
                .map_err(|e| crate::error::EngineError::Ingest(e.to_string()))?;
        }
        Ok(())
    }
}
