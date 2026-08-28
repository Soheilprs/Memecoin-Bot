//! Robinhood primary + fallback HTTP/WS endpoints. No vendor hard-coding. No keys in logs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::Rng;
use serde_json::json;

use crate::error::{EngineError, Result};
use crate::ingest::backoff::redact_url;
use crate::ingest::rpc_json::{hex_u64, http_jsonrpc};
use crate::ingest::rpc_profile::{record, RpcPurpose};

pub const ROBINHOOD_CHAIN_ID: u64 = 4663;
const HEAD_FRESH_SECS: i64 = 45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitKind {
    Throughput,
    Quota,
    Unavailable,
}

impl CircuitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Throughput => "RPC_RATE_LIMIT",
            Self::Quota => "RPC_RATE_LIMIT",
            Self::Unavailable => "RPC_PROVIDER_UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RpcEndpoint {
    pub name: String,
    pub http: String,
    pub ws: Option<String>,
}

impl RpcEndpoint {
    pub fn redacted(&self) -> String {
        format!("{}:{}", self.name, redact_url(&self.http))
    }
}

#[derive(Debug)]
struct Circuit {
    open_until: Option<Instant>,
    kind: Option<CircuitKind>,
    failures: u32,
}

impl Circuit {
    fn open(&self, now: Instant) -> bool {
        self.open_until.is_some_and(|t| now < t)
    }
}

pub struct RpcPool {
    endpoints: Vec<RpcEndpoint>,
    active: AtomicUsize,
    circuit: Mutex<Circuit>,
    chain: &'static str,
}

impl RpcPool {
    pub fn robinhood_from_env() -> Option<Self> {
        let primary_http = std::env::var("RH_RPC_PRIMARY_HTTP")
            .ok()
            .or_else(|| std::env::var("ROBINHOOD_HTTP_URL").ok())
            .filter(|s| !s.is_empty())?;
        let primary_ws = std::env::var("RH_RPC_PRIMARY_WS")
            .ok()
            .or_else(|| std::env::var("ROBINHOOD_WS_URL").ok());
        let mut endpoints = vec![RpcEndpoint {
            name: "primary".into(),
            http: primary_http,
            ws: primary_ws,
        }];
        if let Ok(http) = std::env::var("RH_RPC_FALLBACK_HTTP") {
            if !http.is_empty() {
                endpoints.push(RpcEndpoint {
                    name: "fallback".into(),
                    http,
                    ws: std::env::var("RH_RPC_FALLBACK_WS").ok(),
                });
            }
        }
        Some(Self::new(endpoints, "robinhood"))
    }

    pub fn new(endpoints: Vec<RpcEndpoint>, chain: &'static str) -> Self {
        Self {
            endpoints,
            active: AtomicUsize::new(0),
            circuit: Mutex::new(Circuit {
                open_until: None,
                kind: None,
                failures: 0,
            }),
            chain,
        }
    }

    pub fn from_single(http: String, chain: &'static str) -> Self {
        Self::new(
            vec![RpcEndpoint {
                name: "primary".into(),
                http,
                ws: None,
            }],
            chain,
        )
    }

    pub fn active(&self) -> &RpcEndpoint {
        let i = self.active.load(Ordering::Relaxed);
        &self.endpoints[i.min(self.endpoints.len() - 1)]
    }

    pub fn active_name(&self) -> &str {
        &self.active().name
    }

    pub fn circuit_open(&self) -> bool {
        self.circuit.lock().expect("c").open(Instant::now())
    }

    pub fn circuit_kind(&self) -> Option<CircuitKind> {
        let g = self.circuit.lock().expect("c");
        if g.open(Instant::now()) {
            g.kind
        } else {
            None
        }
    }

    pub fn trip(&self, kind: CircuitKind) {
        let mut g = self.circuit.lock().expect("c");
        g.failures = g.failures.saturating_add(1);
        g.kind = Some(kind);
        let base = match kind {
            CircuitKind::Throughput => Duration::from_secs(1),
            CircuitKind::Quota => Duration::from_secs(60),
            CircuitKind::Unavailable => Duration::from_secs(5),
        };
        let cap = match kind {
            CircuitKind::Quota => Duration::from_secs(300),
            _ => Duration::from_secs(30),
        };
        let exp = base.saturating_mul(1u32 << g.failures.min(8).saturating_sub(1));
        let mut delay = exp.min(cap);
        let jitter_ms = rand::thread_rng().gen_range(0..250u64);
        delay += Duration::from_millis(jitter_ms);
        g.open_until = Some(Instant::now() + delay);
        tracing::warn!(
            provider = %self.active().redacted(),
            kind = kind.as_str(),
            backoff_ms = delay.as_millis() as u64,
            "rpc circuit open"
        );
    }

    pub fn clear_if_elapsed(&self) {
        let mut g = self.circuit.lock().expect("c");
        if g.open_until.is_some_and(|t| Instant::now() >= t) {
            g.open_until = None;
            g.kind = None;
        }
    }

    pub fn note_success(&self) {
        let mut g = self.circuit.lock().expect("c");
        g.failures = 0;
        g.open_until = None;
        g.kind = None;
    }

    pub fn classify_and_trip(&self, err: &str) {
        let s = err.to_ascii_lowercase();
        if s.contains("monthly capacity") || s.contains("capacity limit exceeded") {
            self.trip(CircuitKind::Quota);
        } else if s.contains("429")
            || s.contains("rate limit")
            || s.contains("compute units")
            || s.contains("too many requests")
        {
            self.trip(CircuitKind::Throughput);
        } else if s.contains("timeout") || s.contains("decode") {
            self.trip(CircuitKind::Unavailable);
        }
    }

    pub fn endpoints(&self) -> &[RpcEndpoint] {
        &self.endpoints
    }

    pub async fn call(
        &self,
        http: &reqwest::Client,
        method: &str,
        params: serde_json::Value,
        purpose: RpcPurpose,
    ) -> Result<serde_json::Value> {
        self.clear_if_elapsed();
        if self.circuit_open() {
            return Err(EngineError::Rpc(format!(
                "circuit open: {}",
                self.circuit_kind()
                    .map(|k| k.as_str())
                    .unwrap_or("RPC_PROVIDER_UNAVAILABLE")
            )));
        }
        let n = self.endpoints.len();
        let start = self.active.load(Ordering::Relaxed);
        let mut last = EngineError::Rpc("no rpc endpoint".into());
        for off in 0..n {
            let i = (start + off) % n;
            let ep = &self.endpoints[i];
            let t0 = Instant::now();
            match http_jsonrpc(http, &ep.http, method, params.clone()).await {
                Ok(v) => {
                    record(self.chain, method, purpose, true, t0.elapsed(), Some(200));
                    self.active.store(i, Ordering::Relaxed);
                    self.note_success();
                    return Ok(v);
                }
                Err(e) => {
                    record(self.chain, method, purpose, false, t0.elapsed(), None);
                    let msg = e.to_string();
                    self.classify_and_trip(&msg);
                    last = e;
                    if off + 1 < n && !msg.to_ascii_lowercase().contains("monthly capacity") {
                        tracing::warn!(
                            from = %self.endpoints[i].name,
                            to = %self.endpoints[(i + 1) % n].name,
                            method,
                            "rpc failover"
                        );
                    }
                }
            }
        }
        Err(last)
    }

    pub async fn validate_endpoint(
        &self,
        http: &reqwest::Client,
        ep: &RpcEndpoint,
        expect_chain: u64,
    ) -> Result<u64> {
        let t0 = Instant::now();
        let idv = http_jsonrpc(http, &ep.http, "eth_chainId", json!([])).await;
        record(
            self.chain,
            "eth_chainId",
            RpcPurpose::Head,
            idv.is_ok(),
            t0.elapsed(),
            None,
        );
        let id = hex_u64(&idv?).ok_or_else(|| EngineError::Rpc("bad chainId".into()))?;
        if id != expect_chain {
            return Err(EngineError::Rpc(format!(
                "chain id {id} != expected {expect_chain}"
            )));
        }
        let t1 = Instant::now();
        let headv = http_jsonrpc(http, &ep.http, "eth_blockNumber", json!([])).await;
        record(
            self.chain,
            "eth_blockNumber",
            RpcPurpose::Head,
            headv.is_ok(),
            t1.elapsed(),
            None,
        );
        let head = hex_u64(&headv?).ok_or_else(|| EngineError::Rpc("bad head".into()))?;
        let t2 = Instant::now();
        let blk = http_jsonrpc(
            http,
            &ep.http,
            "eth_getBlockByNumber",
            json!([format!("0x{head:x}"), false]),
        )
        .await;
        record(
            self.chain,
            "eth_getBlockByNumber",
            RpcPurpose::Head,
            blk.is_ok(),
            t2.elapsed(),
            None,
        );
        if let Ok(b) = blk {
            if let Some(ts) = hex_u64(&b["timestamp"]) {
                let lag = (chrono::Utc::now().timestamp() - ts as i64).abs();
                if lag > HEAD_FRESH_SECS {
                    return Err(EngineError::Rpc(format!("head stale lag={lag}s")));
                }
            }
        }
        Ok(head)
    }

    pub async fn block_hash_at(
        &self,
        http: &reqwest::Client,
        ep: &RpcEndpoint,
        block: u64,
    ) -> Result<String> {
        let tag = format!("0x{block:x}");
        let t0 = Instant::now();
        let v = http_jsonrpc(http, &ep.http, "eth_getBlockByNumber", json!([tag, false])).await;
        record(
            self.chain,
            "eth_getBlockByNumber",
            RpcPurpose::PonsCurve,
            v.is_ok(),
            t0.elapsed(),
            None,
        );
        let v = v?;
        v.get("hash")
            .and_then(|h| h.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                EngineError::Rpc("block hash missing on fallback; not substituting latest".into())
            })
    }
}

pub fn classify_circuit(err: &str) -> Option<CircuitKind> {
    let s = err.to_ascii_lowercase();
    if s.contains("monthly capacity") || s.contains("capacity limit exceeded") {
        Some(CircuitKind::Quota)
    } else if s.contains("429") || s.contains("rate limit") || s.contains("compute units") {
        Some(CircuitKind::Throughput)
    } else if s.contains("circuit open") {
        Some(CircuitKind::Unavailable)
    } else {
        None
    }
}
