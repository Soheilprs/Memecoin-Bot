use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::domain::raw_event::{
    normalize_address, CanonicalStatus, DecoderStatus, EvmLog, Finality, RawEvent, RawEventKind,
};
use crate::domain::Chain;
use crate::error::{EngineError, Result};
use crate::ingest::backoff::{redact_url, Backoff};
use crate::ingest::rpc_json::{hex_u64, http_jsonrpc};
use crate::ingest::rpc_profile::{record, RpcPurpose};
use crate::ingest::ResumePlan;
use crate::lab::observation;
use crate::metrics::DiscoveryMetrics;
use crate::registry::{
    BASE_V4_POOL_MANAGER, CLANKER_V4_FACTORY, PONS_V2_FACTORY, ROBINHOOD_V4_POOL_MANAGER,
};
use crate::storage::{ChainHead, Checkpoint, EventStore, IngestGap};
use crate::watch::MarketRegistry;

use super::websocket::EvmWsConfig;

/// Alchemy free tier allows eth_getLogs of at most 10 blocks.
const GETLOGS_MAX_SPAN: u64 = 10;
const OVERLAP_DEFAULT: u64 = 64;
/// Do not replay hours of prior-session history into a live prospective window.
const LIVE_BACKFILL_CAP: u64 = 64;

pub struct EvmLiveCollector<S> {
    pub config: EvmWsConfig,
    pub http_url: String,
    pub store: Arc<S>,
    pub markets: Arc<MarketRegistry>,
    pub metrics: DiscoveryMetrics,
    pub topics: Vec<String>,
}

impl<S: EventStore + Sync + 'static> EvmLiveCollector<S> {
    pub async fn run(
        &self,
        sender: Sender<RawEvent>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
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
                    observation::global().note_collector_down();
                    let msg = err.to_string();
                    if msg.contains("429") || msg.to_ascii_lowercase().contains("capacity") {
                        observation::global().note_rate_limit();
                    }
                    tracing::warn!(
                        chain = %self.config.chain,
                        error = %err,
                        ws = %redact_url(&self.config.ws_url),
                        "evm collector session ended"
                    );
                }
            }
            self.metrics.reconnect(self.config.chain);
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
        let head = eth_block_number(http, &self.http_url).await?;
        let checkpoint = self.store.load_checkpoint(&self.config.ingest_id).await?;
        let plan = ResumePlan::for_evm(checkpoint.as_ref(), head);
        if let Some(from) = plan.from_block {
            let behind = head.saturating_sub(from);
            if behind > LIVE_BACKFILL_CAP {
                let gap = IngestGap {
                    id: None,
                    chain: self.config.chain,
                    source: "evm_ws".into(),
                    stream: self.config.ingest_id.clone(),
                    from_block: Some(from as i64),
                    to_block: Some(head.saturating_sub(GETLOGS_MAX_SPAN) as i64),
                    from_slot: None,
                    to_slot: None,
                    detected_at: Utc::now(),
                    recovered: false,
                    recovered_at: None,
                    reason: "stale_checkpoint_skipped_for_live_session".into(),
                };
                let _ = self.store.insert_gap(&gap).await?;
                self.metrics.stream_gap(self.config.chain);
                let resume = head.saturating_sub(GETLOGS_MAX_SPAN);
                self.backfill(http, sender, resume, head).await?;
            } else if head > from + plan.overlap_blocks + 5 {
                let gap = IngestGap {
                    id: None,
                    chain: self.config.chain,
                    source: "evm_ws".into(),
                    stream: self.config.ingest_id.clone(),
                    from_block: Some(from as i64),
                    to_block: Some(head as i64),
                    from_slot: None,
                    to_slot: None,
                    detected_at: Utc::now(),
                    recovered: false,
                    recovered_at: None,
                    reason: "reconnect_overlap_backfill".into(),
                };
                let id = self.store.insert_gap(&gap).await?;
                self.metrics.stream_gap(self.config.chain);
                self.backfill(http, sender, from, head).await?;
                let _ = self.store.mark_gap_recovered(id).await;
                self.metrics.stream_gap_recovered(self.config.chain);
            } else {
                self.backfill(http, sender, from, head).await?;
            }
        }

        let (ws, _) = connect_async(self.config.ws_url.as_str())
            .await
            .map_err(|e| EngineError::Ingest(format!("ws connect: {e}")))?;
        let (mut write, mut read) = ws.split();
        let mut pending: HashMap<String, VecDeque<RawEvent>> = HashMap::new();
        let logs_sub = json!({
            "jsonrpc":"2.0","id":1,"method":"eth_subscribe",
            "params":["logs", {"topics": [self.topics.clone()]}]
        });
        write
            .send(Message::Text(logs_sub.to_string().into()))
            .await
            .map_err(|e| EngineError::Ingest(e.to_string()))?;
        let heads_sub = json!({
            "jsonrpc":"2.0","id":2,"method":"eth_subscribe","params":["newHeads"]
        });
        write
            .send(Message::Text(heads_sub.to_string().into()))
            .await
            .map_err(|e| EngineError::Ingest(e.to_string()))?;

        let mut last_head = head;
        let connected_at = Instant::now();
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
                        return Err(EngineError::Ingest("websocket closed".into()));
                    };
                    let msg = msg.map_err(|e| EngineError::Ingest(e.to_string()))?;
                    match msg {
                        Message::Text(t) => {
                            self.handle_ws_text(&t, sender, &mut pending, &mut last_head).await?;
                            if connected_at.elapsed() > Duration::from_secs(30) {
                                // session healthy
                            }
                        }
                        Message::Ping(p) => {
                            write.send(Message::Pong(p)).await.ok();
                        }
                        Message::Close(_) => {
                            return Err(EngineError::Ingest("websocket close frame".into()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn handle_ws_text(
        &self,
        text: &str,
        sender: &Sender<RawEvent>,
        pending: &mut HashMap<String, VecDeque<RawEvent>>,
        last_head: &mut u64,
    ) -> Result<()> {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        if v.get("id").is_some() {
            return Ok(());
        }
        let result = v.pointer("/params/result").cloned().unwrap_or(Value::Null);
        if result.get("number").is_some() && result.get("topics").is_none() {
            if let Some(n) = hex_u64(&result["number"]) {
                *last_head = n;
                let ts = hex_u64(&result["timestamp"]).unwrap_or(0);
                let lag = if ts > 0 {
                    (Utc::now().timestamp() - ts as i64).max(0) * 1000
                } else {
                    0
                };
                self.metrics.chain_head_lag_ms(self.config.chain, lag);
                observation::global().note_head(self.config.chain, n);
                let _ = self
                    .store
                    .upsert_head(&ChainHead {
                        chain: self.config.chain,
                        latest_block: Some(n as i64),
                        latest_block_hash: result
                            .get("hash")
                            .and_then(Value::as_str)
                            .map(|s| s.to_string()),
                        latest_slot: None,
                        finalized_block: None,
                        finalized_slot: None,
                        observed_at: Utc::now(),
                        lag_ms: Some(lag),
                    })
                    .await;
                self.flush_pending(pending, sender).await?;
            }
            return Ok(());
        }
        if result.get("topics").is_some() {
            if let Some(raw) = self.log_to_raw(&result, "evm_ws") {
                observation::global().note_log();
                self.route(raw, sender, pending).await?;
            }
        }
        Ok(())
    }

    async fn route(
        &self,
        raw: RawEvent,
        sender: &Sender<RawEvent>,
        pending: &mut HashMap<String, VecDeque<RawEvent>>,
    ) -> Result<()> {
        if let Some(log) = raw.as_evm() {
            let topic = log
                .topics
                .first()
                .map(|t| normalize_address(t))
                .unwrap_or_default();
            if topic == crate::decoders::uniswap_v4::INITIALIZE_TOPIC0 && log.chain == Chain::Base {
                return Ok(());
            }
            if topic == crate::decoders::uniswap_v4::SWAP_TOPIC0 {
                let pool = log
                    .topics
                    .get(1)
                    .map(|t| normalize_address(t))
                    .unwrap_or_default();
                if self.markets.knows_pool(log.chain, &pool) {
                    return emit(sender, raw, &self.metrics).await;
                }
                pending.entry(pool).or_default().push_back(raw);
                if pending.len() > 10_000 {
                    pending.clear();
                    self.metrics.channel_saturated(self.config.chain);
                }
                return Ok(());
            }
        }
        emit(sender, raw, &self.metrics).await
    }

    async fn flush_pending(
        &self,
        pending: &mut HashMap<String, VecDeque<RawEvent>>,
        sender: &Sender<RawEvent>,
    ) -> Result<()> {
        let keys: Vec<String> = pending.keys().cloned().collect();
        for k in keys {
            if self.markets.knows_pool(self.config.chain, &k) {
                if let Some(q) = pending.remove(&k) {
                    for raw in q {
                        emit(sender, raw, &self.metrics).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn backfill(
        &self,
        http: &reqwest::Client,
        sender: &Sender<RawEvent>,
        from: u64,
        to: u64,
    ) -> Result<()> {
        let mut start = from;
        while start <= to {
            let end = (start + GETLOGS_MAX_SPAN - 1).min(to);
            let params = json!([{
                "fromBlock": format!("0x{start:x}"),
                "toBlock": format!("0x{end:x}"),
                "topics": [self.topics.clone()],
            }]);
            let t0 = Instant::now();
            let purpose = if self.config.chain == Chain::Base {
                RpcPurpose::BaseShadow
            } else {
                RpcPurpose::Backfill
            };
            let result = http_jsonrpc(http, &self.http_url, "eth_getLogs", params.clone()).await;
            record(
                self.config.chain.as_str(),
                "eth_getLogs",
                purpose,
                result.is_ok(),
                t0.elapsed(),
                None,
            );
            let result = result?;
            if let Some(arr) = result.as_array() {
                for log in arr {
                    if let Some(raw) = self.log_to_raw(log, "evm_backfill") {
                        if let Some(evm) = raw.as_evm() {
                            let topic = evm
                                .topics
                                .first()
                                .map(|t| normalize_address(t))
                                .unwrap_or_default();
                            if topic == crate::decoders::uniswap_v4::INITIALIZE_TOPIC0
                                && evm.chain == Chain::Base
                            {
                                continue;
                            }
                            if topic == crate::decoders::uniswap_v4::SWAP_TOPIC0 {
                                let pool = evm
                                    .topics
                                    .get(1)
                                    .map(|t| normalize_address(t))
                                    .unwrap_or_default();
                                if !self.markets.knows_pool(evm.chain, &pool)
                                    && !self.is_factory_log(evm)
                                {
                                    continue;
                                }
                            }
                        }
                        emit(sender, raw, &self.metrics).await?;
                    }
                }
            }
            start = end + 1;
        }
        Ok(())
    }

    fn is_factory_log(&self, log: &EvmLog) -> bool {
        let addr = normalize_address(&log.address);
        addr == PONS_V2_FACTORY || addr == CLANKER_V4_FACTORY
    }

    fn log_to_raw(&self, log: &Value, source: &str) -> Option<RawEvent> {
        let address = log.get("address")?.as_str()?.to_string();
        let topics: Vec<String> = log
            .get("topics")?
            .as_array()?
            .iter()
            .filter_map(|t| t.as_str().map(|s| s.to_string()))
            .collect();
        let data = log.get("data")?.as_str()?.to_string();
        let tx = log.get("transactionHash")?.as_str()?.to_string();
        let log_index = hex_u64(log.get("logIndex")?)?;
        let block_number = log.get("blockNumber").and_then(hex_u64);
        let ts = log
            .get("blockTimestamp")
            .and_then(hex_u64)
            .and_then(|t| Utc.timestamp_opt(t as i64, 0).single());
        Some(RawEvent {
            kind: RawEventKind::Evm(EvmLog {
                chain: self.config.chain,
                chain_id: self.config.chain.evm_chain_id().unwrap_or(0),
                address,
                topics,
                data,
                block_number,
                block_hash: log
                    .get("blockHash")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                transaction_hash: tx,
                transaction_index: log.get("transactionIndex").and_then(hex_u64),
                log_index,
                removed: log.get("removed").and_then(Value::as_bool).unwrap_or(false),
                block_timestamp: ts,
                tx_from: None,
            }),
            source: source.into(),
            observed_at: Utc::now(),
            persisted_at: None,
            canonical_status: CanonicalStatus::Canonical,
            finality: Finality::Confirmed,
            decoder_status: DecoderStatus::Pending,
            decoder_version: None,
            error: None,
        })
    }
}

async fn emit(sender: &Sender<RawEvent>, raw: RawEvent, metrics: &DiscoveryMetrics) -> Result<()> {
    match sender.try_send(raw) {
        Ok(()) => Ok(()),
        Err(tokio::sync::mpsc::error::TrySendError::Full(raw)) => {
            metrics.channel_saturated(raw.chain());
            sender
                .send(raw)
                .await
                .map_err(|e| EngineError::Ingest(e.to_string()))
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            Err(EngineError::Ingest("pipeline channel closed".into()))
        }
    }
}

pub async fn eth_block_number(http: &reqwest::Client, url: &str) -> Result<u64> {
    let t0 = Instant::now();
    let v = http_jsonrpc(http, url, "eth_blockNumber", json!([])).await;
    record(
        "evm",
        "eth_blockNumber",
        RpcPurpose::Head,
        v.is_ok(),
        t0.elapsed(),
        None,
    );
    let v = v?;
    hex_u64(&v).ok_or_else(|| EngineError::Rpc("bad block number".into()))
}

pub fn default_topics(chain: Chain) -> Vec<String> {
    match chain {
        Chain::Robinhood => crate::decoders::pons_v2::pons_topic0s()
            .into_iter()
            .map(|s| s.to_string())
            .chain([
                crate::decoders::uniswap_v4::SWAP_TOPIC0.to_string(),
                crate::decoders::uniswap_v4::INITIALIZE_TOPIC0.to_string(),
            ])
            .collect(),
        Chain::Base => vec![
            crate::decoders::clanker_v4::TOKEN_CREATED_TOPIC0.to_string(),
            crate::decoders::uniswap_v4::SWAP_TOPIC0.to_string(),
            crate::decoders::uniswap_v4::INITIALIZE_TOPIC0.to_string(),
        ],
        Chain::Solana => Vec::new(),
    }
}

pub fn default_addresses(chain: Chain) -> Vec<&'static str> {
    match chain {
        Chain::Base => vec![CLANKER_V4_FACTORY, BASE_V4_POOL_MANAGER],
        Chain::Robinhood => vec![PONS_V2_FACTORY, ROBINHOOD_V4_POOL_MANAGER],
        Chain::Solana => Vec::new(),
    }
}

pub fn http_from_ws(ws: &str) -> String {
    ws.replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
}

pub fn _keep_overlap() -> u64 {
    OVERLAP_DEFAULT
}

pub fn checkpoint_from(cp: Option<&Checkpoint>, head: u64) -> ResumePlan {
    ResumePlan::for_evm(cp, head)
}
