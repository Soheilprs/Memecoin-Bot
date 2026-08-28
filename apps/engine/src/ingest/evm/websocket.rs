use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::domain::{Chain, RawEvent};
use crate::error::{EngineError, Result};
use crate::ingest::{ChainIngest, ResumePlan};
use crate::registry::{CLANKER_V4_FACTORY, PONS_V2_FACTORY};
use crate::storage::{Checkpoint, EventStore};

#[derive(Debug, Clone)]
pub struct EvmWsConfig {
    pub chain: Chain,
    pub ws_url: String,
    pub ingest_id: String,
}

pub struct EvmWebsocketIngest<S> {
    pub config: EvmWsConfig,
    pub store: S,
}

impl<S: EventStore> EvmWebsocketIngest<S> {
    pub fn resume_plan(&self, checkpoint: Option<&Checkpoint>, head: u64) -> ResumePlan {
        ResumePlan::for_evm(checkpoint, head)
    }

    pub fn factory_addresses(&self) -> Vec<&'static str> {
        match self.config.chain {
            Chain::Base => vec![CLANKER_V4_FACTORY],
            Chain::Robinhood => vec![PONS_V2_FACTORY],
            Chain::Solana => Vec::new(),
        }
    }
}

#[async_trait]
impl<S: EventStore + Sync> ChainIngest for EvmWebsocketIngest<S> {
    async fn run(&self, _sender: Sender<RawEvent>) -> Result<()> {
        if self.config.ws_url.is_empty() {
            return Err(EngineError::Ingest(format!(
                "{} websocket url is not configured",
                self.config.chain
            )));
        }
        let checkpoint = self.store.load_checkpoint(&self.config.ingest_id).await?;
        let plan = self.resume_plan(checkpoint.as_ref(), 0);
        tracing::info!(
            chain = %self.config.chain,
            ws_url = %self.config.ws_url,
            from_block = ?plan.from_block,
            overlap_blocks = plan.overlap_blocks,
            factories = ?self.factory_addresses(),
            "evm ingest would eth_getLogs overlap then eth_subscribe logs; live ws is phase 2"
        );
        Err(EngineError::Ingest(
            "evm websocket live collector is not enabled in phase 1".into(),
        ))
    }
}
