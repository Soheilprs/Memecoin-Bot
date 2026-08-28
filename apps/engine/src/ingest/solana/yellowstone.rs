use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof;
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, SubscribeUpdate,
};

use crate::domain::{Chain, Finality, RawEvent};
use crate::error::{EngineError, Result};
use crate::ingest::backoff::{redact_url, Backoff};
use crate::ingest::solana::convert::view_from_subscribe_update;
use crate::ingest::solana::health::SolanaSlotTracker;
use crate::ingest::solana::tx::raw_events_from_view;
use crate::ingest::{ChainIngest, ResumePlan};
use crate::metrics::DiscoveryMetrics;
use crate::registry::{PUMPFUN_PROGRAM, PUMPSWAP_PROGRAM};
use crate::storage::{Checkpoint, EventStore};
use crate::watch::MarketRegistry;

#[derive(Debug, Clone)]
pub struct YellowstoneConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub ingest_id: String,
    pub rpc_http: Option<String>,
    pub rpc_ws: Option<String>,
    /// Cost guard: credentials alone never start a paid gRPC stream.
    pub explicitly_enabled: bool,
}

pub struct YellowstoneIngest<S> {
    pub config: YellowstoneConfig,
    pub store: Arc<S>,
    pub markets: Arc<MarketRegistry>,
    pub metrics: DiscoveryMetrics,
    pub shutdown: watch::Receiver<bool>,
    pub slots: Arc<SolanaSlotTracker>,
    pub pool_rx: watch::Receiver<Vec<String>>,
}

impl<S: EventStore> YellowstoneIngest<S> {
    pub fn resume_plan(&self, checkpoint: Option<&Checkpoint>, head_slot: u64) -> ResumePlan {
        ResumePlan::for_solana(checkpoint, head_slot)
    }

    pub fn program_filters() -> Vec<&'static str> {
        vec![PUMPFUN_PROGRAM, PUMPSWAP_PROGRAM]
    }

    fn build_request(&self, from_slot: Option<u64>, pools: &[String]) -> SubscribeRequest {
        let mut transactions = HashMap::new();
        transactions.insert(
            "pumpfun".into(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: None,
                account_include: vec![PUMPFUN_PROGRAM.to_string()],
                ..Default::default()
            },
        );
        if !pools.is_empty() {
            transactions.insert(
                "pumpswap_pools".into(),
                SubscribeRequestFilterTransactions {
                    vote: Some(false),
                    failed: None,
                    account_include: pools.to_vec(),
                    ..Default::default()
                },
            );
        }
        let mut slots = HashMap::new();
        slots.insert(
            "all".into(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(false),
                ..Default::default()
            },
        );
        SubscribeRequest {
            slots,
            transactions,
            commitment: Some(CommitmentLevel::Processed as i32),
            from_slot,
            ..Default::default()
        }
    }
}

#[async_trait::async_trait]
impl<S: EventStore + Sync + 'static> ChainIngest for YellowstoneIngest<S> {
    async fn run(&self, sender: Sender<RawEvent>) -> Result<()> {
        if !self.config.explicitly_enabled {
            return Err(EngineError::Ingest(
                "Yellowstone gRPC is disabled unless SOLANA_MODE=yellowstone (cost guard)".into(),
            ));
        }
        if self.config.endpoint.is_empty() {
            return Err(EngineError::Ingest(
                "SOLANA_GRPC_URL is required for the Yellowstone live path".into(),
            ));
        }
        let mut backoff = Backoff::default();
        let mut shutdown = self.shutdown.clone();
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            match self.session(&sender, &mut shutdown).await {
                Ok(()) => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        endpoint = %redact_url(&self.config.endpoint),
                        "yellowstone session ended"
                    );
                }
            }
            self.metrics.reconnect(Chain::Solana);
            let delay = backoff.next_delay();
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { return Ok(()); }
                }
            }
        }
    }
}

impl<S: EventStore + Sync + 'static> YellowstoneIngest<S> {
    async fn session(
        &self,
        sender: &Sender<RawEvent>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let mut builder = GeyserGrpcClient::build_from_shared(self.config.endpoint.clone())
            .map_err(|e| EngineError::Ingest(format!("yellowstone builder: {e}")))?
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|e| EngineError::Ingest(format!("yellowstone tls: {e}")))?;
        if let Some(token) = &self.config.x_token {
            builder = builder
                .x_token(Some(token.clone()))
                .map_err(|e| EngineError::Ingest(format!("yellowstone x-token: {e}")))?;
        }
        let mut client = builder
            .connect()
            .await
            .map_err(|e| EngineError::Ingest(format!("yellowstone connect: {e}")))?;

        let checkpoint = self.store.load_checkpoint(&self.config.ingest_id).await?;
        let persisted = checkpoint
            .as_ref()
            .and_then(|c| c.last_slot.or(c.last_confirmed_slot))
            .map(|s| s as u64);
        let overlap = checkpoint
            .as_ref()
            .map(|c| c.overlap_slots.max(1) as u64)
            .unwrap_or(32);
        let from_slot = persisted.map(|s| s.saturating_sub(overlap));
        if from_slot.is_some() {
            DiscoveryMetrics::solana_repair_attempt();
        }
        let pools = self.markets.solana_pools();
        let request = self.build_request(from_slot, &pools);
        tracing::info!(
            endpoint = %redact_url(&self.config.endpoint),
            from_slot = ?from_slot,
            pumpswap_pools = pools.len(),
            "yellowstone gRPC subscribe (processed) Pump.fun + watched PumpSwap pools"
        );

        let (mut sink, mut stream) =
            match client.subscribe_with_request(Some(request.clone())).await {
                Ok(pair) => pair,
                Err(_) => {
                    // Some hosts accept subscribe_with_request; others require subscribe() then send.
                    let (mut sink, stream) = client
                        .subscribe()
                        .await
                        .map_err(|e| EngineError::Ingest(format!("yellowstone subscribe: {e}")))?;
                    sink.send(request)
                        .await
                        .map_err(|e| EngineError::Ingest(format!("yellowstone send: {e}")))?;
                    (sink, stream)
                }
            };

        if from_slot.is_some() {
            DiscoveryMetrics::solana_repair_success();
        }

        let mut pool_rx = self.pool_rx.clone();
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                }
                changed = pool_rx.changed() => {
                    if changed.is_err() {
                        continue;
                    }
                    let pools = pool_rx.borrow().clone();
                    let req = self.build_request(None, &pools);
                    if let Err(err) = sink.send(req).await {
                        return Err(EngineError::Ingest(format!("resubscribe pools: {err}")));
                    }
                    tracing::info!(pools = pools.len(), "updated yellowstone PumpSwap pool filters");
                }
                next = stream.next() => {
                    let Some(item) = next else {
                        return Err(EngineError::Ingest("yellowstone stream closed".into()));
                    };
                    let update = item.map_err(|e| EngineError::Ingest(format!("yellowstone stream: {e}")))?;
                    self.on_update(update, sender).await?;
                }
            }
        }
    }

    async fn on_update(&self, update: SubscribeUpdate, sender: &Sender<RawEvent>) -> Result<()> {
        match update.update_oneof.as_ref() {
            Some(UpdateOneof::Slot(slot)) => {
                let s = slot.slot;
                // 0 processed, 1 confirmed, 2 finalized in geyser SlotStatus (varies by version)
                let status = slot.status;
                if status == 0 {
                    self.slots.note_head(s);
                } else if status == 1 {
                    self.slots.note_confirmed(s);
                } else if status == 2 {
                    self.slots.note_finalized(s);
                } else {
                    self.slots.note_head(s);
                }
                self.slots
                    .flush_gaps(self.store.as_ref(), "slot_skip")
                    .await;
                return Ok(());
            }
            Some(UpdateOneof::Ping(_)) => return Ok(()),
            _ => {}
        }
        let Some(view) =
            view_from_subscribe_update(&update, "yellowstone", Utc::now(), Finality::Processed)
        else {
            return Ok(());
        };
        if let Some(slot) = view.slot {
            self.slots.note_received(slot);
        }
        if view.block_time.is_none() {
            if let Some(ct) = update.created_at {
                // created_at is stream time, not chain time; leave chain_time empty.
                let _ = ct;
            }
        }
        let events = raw_events_from_view(&view);
        for raw in events {
            match sender.try_send(raw) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(raw)) => {
                    self.metrics.channel_saturated(Chain::Solana);
                    sender
                        .send(raw)
                        .await
                        .map_err(|e| EngineError::Ingest(e.to_string()))?;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    return Err(EngineError::Ingest("pipeline closed".into()));
                }
            }
        }
        Ok(())
    }
}
