use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::domain::{Finality, RawEvent};
use crate::error::{EngineError, Result};
use crate::ingest::backoff::{redact_url, Backoff};
use crate::ingest::rpc_json::http_jsonrpc;
use crate::ingest::solana::parse::raw_events_from_get_transaction;
use crate::ingest::solana::yellowstone::YellowstoneConfig;
use crate::metrics::DiscoveryMetrics;
use crate::registry::PUMPFUN_PROGRAM;
use crate::storage::{ChainHead, EventStore, IngestGap};

pub struct SolanaRpcCollector<S> {
    pub config: YellowstoneConfig,
    pub rpc_http: String,
    pub rpc_ws: String,
    pub store: Arc<S>,
    pub metrics: DiscoveryMetrics,
}

impl<S: EventStore + Sync + 'static> SolanaRpcCollector<S> {
    pub async fn run(
        &self,
        sender: Sender<RawEvent>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(|e| EngineError::Rpc(e.to_string()))?;
        let mut backoff = Backoff::default();
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            match self.session(&http, &sender, &mut shutdown).await {
                Ok(()) => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        rpc = %redact_url(&self.rpc_http),
                        "solana rpc collector session ended"
                    );
                }
            }
            self.metrics.reconnect(crate::domain::Chain::Solana);
            let delay = backoff.next_delay();
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { return Ok(()); }
                }
            }
        }
    }

    async fn session(
        &self,
        http: &reqwest::Client,
        sender: &Sender<RawEvent>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let slot = slot_now(http, &self.rpc_http, "confirmed").await?;
        let checkpoint = self.store.load_checkpoint(&self.config.ingest_id).await?;
        if let Some(cp) = checkpoint.as_ref() {
            if let Some(last) = cp.last_slot {
                let from = (last as u64).saturating_sub(cp.overlap_slots.max(1) as u64);
                if slot > from + 64 {
                    let gap = IngestGap {
                        id: None,
                        chain: crate::domain::Chain::Solana,
                        source: "solana_rpc".into(),
                        stream: self.config.ingest_id.clone(),
                        from_block: None,
                        to_block: None,
                        from_slot: Some(from as i64),
                        to_slot: Some(slot as i64),
                        detected_at: Utc::now(),
                        recovered: false,
                        recovered_at: None,
                        reason: "reconnect_slot_gap".into(),
                    };
                    let id = self.store.insert_gap(&gap).await?;
                    self.metrics.stream_gap(crate::domain::Chain::Solana);
                    match self.backfill_signatures(http, sender, from).await {
                        Ok(()) => {
                            let _ = self.store.mark_gap_recovered(id).await;
                            self.metrics
                                .stream_gap_recovered(crate::domain::Chain::Solana);
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "solana backfill incomplete; gap remains");
                        }
                    }
                } else {
                    let _ = self.backfill_signatures(http, sender, from).await;
                }
            }
        }

        let ws_url = if self.rpc_ws.is_empty() {
            http_to_ws(&self.rpc_http)
        } else {
            self.rpc_ws.clone()
        };
        let (ws, _) = connect_async(ws_url.as_str())
            .await
            .map_err(|e| EngineError::Ingest(format!("solana ws: {e}")))?;
        let (mut write, mut read) = ws.split();
        let sub = json!({
            "jsonrpc":"2.0","id":1,"method":"logsSubscribe",
            "params":[{"mentions":[PUMPFUN_PROGRAM]},{"commitment":"confirmed"}]
        });
        write
            .send(Message::Text(sub.to_string().into()))
            .await
            .map_err(|e| EngineError::Ingest(e.to_string()))?;
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        let _ = write.close().await;
                        return Ok(());
                    }
                }
                msg = read.next() => {
                    let Some(msg) = msg else {
                        return Err(EngineError::Ingest("solana ws closed".into()));
                    };
                    let msg = msg.map_err(|e| EngineError::Ingest(e.to_string()))?;
                    match msg {
                        Message::Text(t) => {
                            self.on_text(http, sender, &t).await?;
                        }
                        Message::Ping(p) => { let _ = write.send(Message::Pong(p)).await; }
                        Message::Close(_) => return Err(EngineError::Ingest("solana ws close".into())),
                        _ => {}
                    }
                }
            }
        }
    }

    async fn on_text(
        &self,
        http: &reqwest::Client,
        sender: &Sender<RawEvent>,
        text: &str,
    ) -> Result<()> {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        if let Some(n) = v
            .pointer("/params/result/context/slot")
            .and_then(Value::as_u64)
        {
            let _ = self
                .store
                .upsert_head(&ChainHead {
                    chain: crate::domain::Chain::Solana,
                    latest_block: None,
                    latest_block_hash: None,
                    latest_slot: Some(n as i64),
                    finalized_block: None,
                    finalized_slot: None,
                    observed_at: Utc::now(),
                    lag_ms: None,
                })
                .await;
        }
        let Some(sig) = v
            .pointer("/params/result/value/signature")
            .and_then(Value::as_str)
        else {
            return Ok(());
        };
        self.fetch_and_emit(http, sender, sig, "solana_ws").await
    }

    async fn fetch_and_emit(
        &self,
        http: &reqwest::Client,
        sender: &Sender<RawEvent>,
        sig: &str,
        source: &str,
    ) -> Result<()> {
        let params = json!([
            sig,
            {"encoding":"json","maxSupportedTransactionVersion":0,"commitment":"confirmed"}
        ]);
        let tx = match http_jsonrpc(http, &self.rpc_http, "getTransaction", params).await {
            Ok(v) => v,
            Err(err) => {
                tracing::debug!(error = %err, signature = %sig, "getTransaction failed");
                return Ok(());
            }
        };
        let events = raw_events_from_get_transaction(&tx, source, Utc::now(), Finality::Confirmed);
        for raw in events {
            match sender.try_send(raw) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(raw)) => {
                    self.metrics.channel_saturated(crate::domain::Chain::Solana);
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

    async fn backfill_signatures(
        &self,
        http: &reqwest::Client,
        sender: &Sender<RawEvent>,
        _from_slot: u64,
    ) -> Result<()> {
        let params = json!([
            PUMPFUN_PROGRAM,
            {"limit": 50, "commitment": "confirmed"}
        ]);
        let sigs = http_jsonrpc(http, &self.rpc_http, "getSignaturesForAddress", params).await?;
        let Some(arr) = sigs.as_array() else {
            return Err(EngineError::Rpc(
                "getSignaturesForAddress cannot replay exact slot range; gap recorded".into(),
            ));
        };
        for s in arr.iter().rev() {
            if let Some(sig) = s.get("signature").and_then(Value::as_str) {
                self.fetch_and_emit(http, sender, sig, "solana_backfill")
                    .await?;
            }
        }
        Ok(())
    }
}

async fn slot_now(http: &reqwest::Client, url: &str, commitment: &str) -> Result<u64> {
    let v = http_jsonrpc(http, url, "getSlot", json!([{"commitment": commitment}])).await?;
    v.as_u64()
        .ok_or_else(|| EngineError::Rpc("bad slot".into()))
}

fn http_to_ws(http: &str) -> String {
    http.replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}
