use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::decoders::{DecodeOutcome, DecoderRegistry};
use crate::domain::{
    CanonicalEvent, CanonicalStatus, DecoderStatus, LifecycleObserved, RawEvent, TokenDiscovered,
    TradeObserved,
};
use crate::error::Result;
use crate::ingest::solana::health::SolanaSlotTracker;
use crate::metrics::DiscoveryMetrics;
use crate::normalize::attach_raw_timestamps;
use crate::state::{StateEngine, TokenKey};
use crate::storage::{Checkpoint, EventStore, InsertRaw};
use crate::watch::{MarketRef, MarketRegistry};

pub struct DiscoveryPipeline<S> {
    pub store: Arc<S>,
    pub registry: DecoderRegistry,
    pub markets: Arc<MarketRegistry>,
    pub discovered_tx: Sender<TokenDiscovered>,
    pub trade_tx: Sender<TradeObserved>,
    pub lifecycle_tx: Sender<LifecycleObserved>,
    pub metrics: DiscoveryMetrics,
    pub ingest_id: String,
    pub slots: Option<Arc<SolanaSlotTracker>>,
    pub pool_tx: Option<tokio::sync::watch::Sender<Vec<String>>>,
    pub state: Option<Arc<Mutex<StateEngine>>>,
}

impl<S: EventStore> DiscoveryPipeline<S> {
    pub async fn handle(&self, mut raw: RawEvent) -> Result<HandleResult> {
        let ingest_started = Instant::now();
        if raw.observed_at.timestamp() == 0 {
            raw.observed_at = Utc::now();
        }
        self.metrics.raw_received(&raw);
        if raw.source.contains("backfill") || raw.source.ends_with(":backfill") {
            self.metrics.backfill_event(&raw);
        } else {
            self.metrics.live_event(&raw);
        }
        let event_id = raw.event_id();
        match self.store.insert_raw(&raw).await? {
            InsertRaw::Duplicate => {
                self.metrics.duplicate(&raw);
                return Ok(HandleResult::Duplicate { event_id });
            }
            InsertRaw::Inserted => {}
        }
        let persist_at = Utc::now();
        let _ = self.store.set_persisted_at(&event_id, persist_at).await;
        raw.persisted_at = Some(persist_at);

        if let crate::domain::raw_event::RawEventKind::Evm(log) = &raw.kind {
            if log.removed {
                self.store.mark_orphaned(&event_id).await?;
                self.metrics.orphaned(&raw);
                return Ok(HandleResult::Orphaned { event_id });
            }
        }

        match self.registry.decode(&raw) {
            Ok(DecodeOutcome::Events(events)) => {
                let mut discovered = None;
                let mut trades = 0usize;
                let mut lifecycle = 0usize;
                for mut event in events {
                    match &mut event {
                        CanonicalEvent::TokenDiscovered(token) => {
                            attach_raw_timestamps(&raw, token.as_mut());
                            token.persisted_at = Some(persist_at);
                        }
                        CanonicalEvent::Trade(trade) => {
                            self.enrich_trade(trade);
                            trade.persisted_at = Some(persist_at);
                            trade.observed_at = raw.observed_at;
                        }
                        CanonicalEvent::Lifecycle(life) => {
                            self.enrich_lifecycle(life);
                            life.persisted_at = Some(persist_at);
                            life.observed_at = raw.observed_at;
                        }
                    }
                    self.apply_state(event.clone()).await;
                    match event {
                        CanonicalEvent::TokenDiscovered(token) => {
                            self.store
                                .mark_decoder(
                                    &event_id,
                                    DecoderStatus::Success,
                                    Some(&token.decoder_version),
                                    None,
                                )
                                .await?;
                            self.store.insert_discovered(&token).await?;
                            let market = self.register_from_token(&token);
                            let _ = self
                                .store
                                .upsert_watched_market(&market, Some(&token.raw_event_id))
                                .await;
                            self.metrics.decode_success(&token);
                            let _ = self.discovered_tx.try_send((*token).clone());
                            discovered = Some(token);
                        }
                        CanonicalEvent::Trade(trade) => {
                            self.store
                                .mark_decoder(
                                    &event_id,
                                    DecoderStatus::Success,
                                    Some(&trade.decoder_version),
                                    None,
                                )
                                .await?;
                            self.store.insert_trade(&trade).await?;
                            self.metrics.trade(&trade);
                            let _ = self.trade_tx.try_send((*trade).clone());
                            trades += 1;
                        }
                        CanonicalEvent::Lifecycle(life) => {
                            self.store
                                .mark_decoder(
                                    &event_id,
                                    DecoderStatus::Success,
                                    Some(&life.decoder_version),
                                    None,
                                )
                                .await?;
                            self.store.insert_lifecycle(&life).await?;
                            if let Some(market) = self.register_from_lifecycle(&life) {
                                let _ = self
                                    .store
                                    .upsert_watched_market(&market, Some(&life.raw_event_id))
                                    .await;
                            }
                            self.metrics.lifecycle(&life);
                            let _ = self.lifecycle_tx.try_send((*life).clone());
                            lifecycle += 1;
                        }
                    }
                }
                self.metrics.record_lags(&raw, persist_at, ingest_started);
                if let Some(slot) = raw.slot() {
                    if let Some(t) = &self.slots {
                        t.note_persisted(slot as u64);
                    }
                }
                self.note_checkpoint(&raw).await;
                if let Some(token) = discovered {
                    Ok(HandleResult::Discovered(token))
                } else {
                    Ok(HandleResult::Canonical {
                        event_id,
                        trades,
                        lifecycle,
                    })
                }
            }
            Ok(DecodeOutcome::Unknown) => {
                self.store
                    .mark_decoder(&event_id, DecoderStatus::Unknown, None, None)
                    .await?;
                self.metrics.unknown(&raw);
                self.note_checkpoint(&raw).await;
                Ok(HandleResult::Unknown { event_id })
            }
            Err(err) => {
                self.store
                    .mark_decoder(
                        &event_id,
                        DecoderStatus::Error,
                        raw.decoder_version.as_deref(),
                        Some(&err.to_string()),
                    )
                    .await?;
                self.metrics.decode_failure(&raw);
                self.note_checkpoint(&raw).await;
                Ok(HandleResult::DecodeError {
                    event_id,
                    error: err.to_string(),
                })
            }
        }
    }

    fn enrich_trade(&self, trade: &mut TradeObserved) {
        if trade.token_address.is_empty() {
            if let Some(curve) = trade.curve.as_deref() {
                if let Some(m) = self.markets.by_curve(trade.chain, curve) {
                    trade.token_address = m.token_address;
                    trade.launchpad = m.launchpad;
                    if let Some(q) = m.quote_asset {
                        trade.quote_asset = q;
                    }
                }
            }
        }
        if trade.token_address.is_empty() {
            if let Some(pool) = trade.pool.as_deref() {
                if let Some(m) = self.markets.by_pool(trade.chain, pool) {
                    trade.token_address = m.token_address;
                    trade.launchpad = m.launchpad;
                    if let Some(q) = m.quote_asset {
                        trade.quote_asset = q;
                    }
                }
            }
        }
        if trade.token_address.is_empty() {
            if let Some(curve) = trade.curve.as_deref() {
                trade.token_address = curve.to_string();
                trade.metadata["token_unresolved"] = serde_json::json!(true);
            } else if let Some(pool) = trade.pool.as_deref() {
                trade.token_address = pool.to_string();
                trade.metadata["token_unresolved"] = serde_json::json!(true);
            }
        }
    }

    fn enrich_lifecycle(&self, life: &mut LifecycleObserved) {
        if !life.token_address.is_empty() {
            return;
        }
        if let Some(curve) = life.curve.as_deref() {
            if let Some(m) = self.markets.by_curve(life.chain, curve) {
                life.token_address = m.token_address;
                life.launchpad = m.launchpad;
            }
        }
        if life.token_address.is_empty() {
            if let Some(pool) = life.pool.as_deref() {
                if let Some(m) = self.markets.by_pool(life.chain, pool) {
                    life.token_address = m.token_address;
                    life.launchpad = m.launchpad;
                }
            }
        }
    }

    fn register_from_token(&self, token: &TokenDiscovered) -> MarketRef {
        let market = MarketRef {
            chain: token.chain,
            launchpad: token.launchpad,
            token_address: token.token_address.clone(),
            curve: token.curve.clone(),
            pool: token.pool.clone(),
            pool_id: token.pool.clone(),
            quote_asset: token.quote_asset.clone(),
        };
        self.markets.register(market.clone());
        market
    }

    fn register_from_lifecycle(&self, life: &LifecycleObserved) -> Option<MarketRef> {
        if life.token_address.is_empty() {
            return None;
        }
        let market = MarketRef {
            chain: life.chain,
            launchpad: life.launchpad,
            token_address: life.token_address.clone(),
            curve: life.curve.clone(),
            pool: life.pool.clone(),
            pool_id: life.pool.clone(),
            quote_asset: None,
        };
        self.markets.register(market.clone());
        if life.chain == crate::domain::Chain::Solana {
            if let Some(tx) = &self.pool_tx {
                let _ = tx.send(self.markets.solana_pools());
            }
        }
        Some(market)
    }

    async fn note_checkpoint(&self, raw: &RawEvent) {
        let ingest_id = match raw.chain() {
            crate::domain::Chain::Solana => "solana:pumpfun",
            crate::domain::Chain::Base => "base:live",
            crate::domain::Chain::Robinhood => "robinhood:live",
        };
        let mut cp = Checkpoint::new(ingest_id, raw.chain());
        cp.last_block = raw.block_number();
        cp.last_block_hash = raw.block_hash().map(|s| s.to_string());
        cp.last_slot = raw.slot();
        if let Some(t) = &self.slots {
            cp.last_confirmed_slot = Some(t.confirmed() as i64).filter(|v| *v > 0);
            cp.last_finalized_slot = Some(t.finalized() as i64).filter(|v| *v > 0);
        }
        cp.last_signature = Some(raw.tx_hash().to_string());
        let _ = self.store.save_checkpoint(&cp).await;
    }

    async fn apply_state(&self, event: CanonicalEvent) {
        let Some(eng) = &self.state else {
            return;
        };
        {
            let mut g = match eng.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.push_event(event);
        }
        self.flush_rebuilds().await;
    }

    async fn flush_rebuilds(&self) {
        let Some(eng) = &self.state else {
            return;
        };
        let keys = {
            let mut g = match eng.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.pending_rebuilds()
        };
        for key in keys {
            let _ = self.rebuild_token_state(key).await;
        }
    }

    async fn rebuild_token_state(&self, key: TokenKey) -> Result<()> {
        self.store
            .mark_snapshots_superseded(key.chain, &key.token)
            .await?;
        let mut events = Vec::new();
        if let Some(d) = self
            .store
            .load_token_discovered(key.chain, &key.token)
            .await?
        {
            events.push(CanonicalEvent::TokenDiscovered(Box::new(d)));
        }
        for t in self.store.load_token_trades(key.chain, &key.token).await? {
            if t.canonical_status != CanonicalStatus::Orphaned {
                events.push(CanonicalEvent::Trade(Box::new(t)));
            }
        }
        for l in self
            .store
            .load_token_lifecycle(key.chain, &key.token)
            .await?
        {
            if l.canonical_status != CanonicalStatus::Orphaned {
                events.push(CanonicalEvent::Lifecycle(Box::new(l)));
            }
        }
        let snaps = {
            let mut g = self
                .state
                .as_ref()
                .expect("state")
                .lock()
                .expect("state lock");
            g.rebuild_token(key.clone(), events)
        };
        for snap in snaps {
            let id = self.store.insert_snapshot(&snap).await?;
            let _ = self
                .store
                .upsert_current_state(
                    snap.chain,
                    &snap.token_address,
                    Some(id),
                    snap.lifecycle_state.as_str(),
                    Some(snap.snapshot_time),
                    snap.as_of_event_id.as_deref(),
                    snap.data_quality,
                )
                .await;
        }
        Ok(())
    }

    pub async fn mark_removed(&self, event_id: &str, chain: crate::domain::Chain) -> Result<bool> {
        let ok = self.store.mark_orphaned(event_id).await?;
        if ok {
            self.metrics.orphaned_id(chain);
            if let Some(raw) = self.store.get_raw(event_id).await? {
                if let Some(eng) = &self.state {
                    if let Ok(mut g) = eng.lock() {
                        g.note_orphan(chain, raw.tx_hash());
                    }
                }
                if let Some(t) = self.store.get_trade(event_id).await? {
                    if let Some(eng) = &self.state {
                        eng.lock().unwrap().note_orphan(t.chain, &t.token_address);
                    }
                    self.flush_rebuilds().await;
                }
            }
        }
        Ok(ok)
    }

    pub async fn run_loop(&self, mut rx: Receiver<RawEvent>) {
        while let Some(raw) = rx.recv().await {
            if let Err(err) = self.handle(raw).await {
                tracing::error!(error = %err, "pipeline handle failed");
                self.metrics.db_write_failure();
            }
        }
    }
}

#[derive(Debug)]
pub enum HandleResult {
    Discovered(Box<TokenDiscovered>),
    Canonical {
        event_id: String,
        trades: usize,
        lifecycle: usize,
    },
    Duplicate {
        event_id: String,
    },
    Unknown {
        event_id: String,
    },
    DecodeError {
        event_id: String,
        error: String,
    },
    Orphaned {
        event_id: String,
    },
}
