//! Read-only Pons V2 curve state via verified getters. No sendTransaction.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use crate::domain::Chain;
use crate::error::{EngineError, Result};
use crate::ingest::rpc_json::{hex_u64, http_jsonrpc};
use crate::metrics::DiscoveryMetrics;
use crate::state::amt::{parse_u256, u256_dec};
use crate::state::pons_curve::{
    decode_abi_bool, decode_abi_words, PonsCurveState, PonsCurveStateQuality, PonsCurveStatus,
    PONS_CURVE_ABI_VERSION, PONS_CURVE_SOURCE,
};

pub const GET_RESERVES: &str = "0x0902f1ac";
pub const REAL_QUOTE_RESERVE: &str = "0x4f1f58fd";
pub const PHANTOM_QUOTE: &str = "0xc57eadfc";
pub const GRADUATION_THRESHOLD: &str = "0x8b0bc501";
pub const GRADUATED: &str = "0xe7c2b772";
pub const FEE_BPS: &str = "0x24a9d853";
pub const CREATOR_TAX_BPS: &str = "0xc1bb8901";
pub const SELLABLE_TOKENS: &str = "0x808bcddc";
pub const READY_TO_GRADUATE: &str = "0xc68360a5";

const MAX_INFLIGHT: usize = 2;
const CALL_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveReadErrorKind {
    Timeout,
    RateLimit,
    NotFound,
    Invalid,
    Other,
}

impl CurveReadErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "PROVIDER_TIMEOUT",
            Self::RateLimit => "PROVIDER_RATE_LIMIT",
            Self::NotFound => "CURVE_NOT_FOUND",
            Self::Invalid => "INVALID_CURVE_STATE",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CurveReadError {
    pub kind: CurveReadErrorKind,
    pub message: String,
}

impl CurveReadError {
    pub fn new(kind: CurveReadErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn reason(&self) -> String {
        format!("{}: {}", self.kind.as_str(), self.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalCallCapability {
    pub supported: bool,
    pub tested_head: u64,
    pub tested_offsets: Vec<u64>,
    pub failed_offset: Option<u64>,
    pub note: String,
}

#[derive(Debug, Clone)]
pub enum ReaderMode {
    Live(String),
    Mock(Box<PonsCurveState>),
    Fail(CurveReadErrorKind, String),
}

#[derive(Clone)]
pub struct PonsCurveReader {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    mode: ReaderMode,
    cache: Mutex<HashMap<(String, u64), PonsCurveState>>,
    sem: Semaphore,
    historical: Mutex<Option<HistoricalCallCapability>>,
    reads: AtomicU64,
    failures: AtomicU64,
    cache_hits: AtomicU64,
    rate_limits: AtomicU64,
}

impl PonsCurveReader {
    pub fn new(http_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(CALL_TIMEOUT)
            .build()
            .map_err(|e| EngineError::Rpc(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                mode: ReaderMode::Live(http_url.into()),
                cache: Mutex::new(HashMap::new()),
                sem: Semaphore::new(MAX_INFLIGHT),
                historical: Mutex::new(None),
                reads: AtomicU64::new(0),
                failures: AtomicU64::new(0),
                cache_hits: AtomicU64::new(0),
                rate_limits: AtomicU64::new(0),
            }),
        })
    }

    pub fn mock(state: PonsCurveState) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                mode: ReaderMode::Mock(Box::new(state)),
                cache: Mutex::new(HashMap::new()),
                sem: Semaphore::new(MAX_INFLIGHT),
                historical: Mutex::new(Some(HistoricalCallCapability {
                    supported: true,
                    tested_head: 0,
                    tested_offsets: vec![0, 10, 100],
                    failed_offset: None,
                    note: "mock reader".into(),
                })),
                reads: AtomicU64::new(0),
                failures: AtomicU64::new(0),
                cache_hits: AtomicU64::new(0),
                rate_limits: AtomicU64::new(0),
            }),
        }
    }

    pub fn failing(kind: CurveReadErrorKind, message: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                mode: ReaderMode::Fail(kind, message.into()),
                cache: Mutex::new(HashMap::new()),
                sem: Semaphore::new(MAX_INFLIGHT),
                historical: Mutex::new(None),
                reads: AtomicU64::new(0),
                failures: AtomicU64::new(0),
                cache_hits: AtomicU64::new(0),
                rate_limits: AtomicU64::new(0),
            }),
        }
    }

    pub fn reads(&self) -> u64 {
        self.inner.reads.load(Ordering::Relaxed)
    }
    pub fn failures(&self) -> u64 {
        self.inner.failures.load(Ordering::Relaxed)
    }
    pub fn cache_hits(&self) -> u64 {
        self.inner.cache_hits.load(Ordering::Relaxed)
    }
    pub fn rate_limits(&self) -> u64 {
        self.inner.rate_limits.load(Ordering::Relaxed)
    }

    pub fn historical_capability(&self) -> Option<HistoricalCallCapability> {
        self.inner.historical.lock().expect("hist").clone()
    }

    pub async fn head_block(&self) -> std::result::Result<u64, CurveReadError> {
        match &self.inner.mode {
            ReaderMode::Live(url) => {
                let v = self
                    .rpc(url, "eth_blockNumber", json!([]))
                    .await
                    .map_err(|e| classify_rpc(&e.to_string()))?;
                hex_u64(&v)
                    .ok_or_else(|| CurveReadError::new(CurveReadErrorKind::Invalid, "bad head"))
            }
            ReaderMode::Mock(s) => Ok(s.block_number.unwrap_or(1)),
            ReaderMode::Fail(k, m) => Err(CurveReadError::new(*k, m.clone())),
        }
    }

    /// Block-pinned getter read. Cache key is (curve, block_number).
    pub async fn read(
        &self,
        token: &str,
        curve: &str,
        block: Option<u64>,
    ) -> std::result::Result<PonsCurveState, CurveReadError> {
        let _permit = self
            .inner
            .sem
            .acquire()
            .await
            .map_err(|e| CurveReadError::new(CurveReadErrorKind::Other, e.to_string()))?;
        self.inner.reads.fetch_add(1, Ordering::Relaxed);
        DiscoveryMetrics::pons_curve_state_read();

        match &self.inner.mode {
            ReaderMode::Mock(s) => {
                let mut out = (**s).clone();
                out.token = token.to_ascii_lowercase();
                out.curve = curve.to_ascii_lowercase();
                if let Some(b) = block {
                    out.block_number = Some(b);
                }
                return Ok(out);
            }
            ReaderMode::Fail(k, m) => {
                self.note_failure(*k);
                return Err(CurveReadError::new(*k, m.clone()));
            }
            ReaderMode::Live(_) => {}
        }

        let head = match block {
            Some(b) => b,
            None => self.head_block().await?,
        };
        let curve_n = curve.to_ascii_lowercase();
        if let Some(hit) = self
            .inner
            .cache
            .lock()
            .expect("cache")
            .get(&(curve_n.clone(), head))
            .cloned()
        {
            self.inner.cache_hits.fetch_add(1, Ordering::Relaxed);
            DiscoveryMetrics::pons_curve_state_cache_hit();
            return Ok(hit);
        }

        let started = Instant::now();
        let result = self.read_live(&curve_n, token, head).await;
        DiscoveryMetrics::pons_curve_state_latency_ms(started.elapsed().as_millis() as i64);
        match result {
            Ok(state) => {
                self.inner
                    .cache
                    .lock()
                    .expect("cache")
                    .insert((curve_n, head), state.clone());
                Ok(state)
            }
            Err(e) => {
                self.note_failure(e.kind);
                Err(e)
            }
        }
    }

    async fn read_live(
        &self,
        curve: &str,
        token: &str,
        block: u64,
    ) -> std::result::Result<PonsCurveState, CurveReadError> {
        let url = match &self.inner.mode {
            ReaderMode::Live(u) => u.clone(),
            _ => unreachable!(),
        };
        let tag = format!("0x{block:x}");
        let code = self.eth_call_raw(&url, curve, "0x", Some(&tag)).await.ok();
        // getCode separately
        let code_v = self.rpc(&url, "eth_getCode", json!([curve, tag])).await;
        match code_v {
            Ok(Value::String(s)) if s == "0x" || s == "0x0" || s.len() <= 2 => {
                return Err(CurveReadError::new(
                    CurveReadErrorKind::NotFound,
                    "eth_getCode empty",
                ));
            }
            Err(e) => {
                let c = classify_rpc(&e.to_string());
                if matches!(
                    c.kind,
                    CurveReadErrorKind::Timeout | CurveReadErrorKind::RateLimit
                ) {
                    return Err(c);
                }
            }
            _ => {}
        }
        let _ = code;

        let hist = self.inner.historical.lock().expect("hist").clone();
        let (tag_used, quality) = match hist {
            Some(h) if !h.supported => {
                ("latest".to_string(), PonsCurveStateQuality::LiveLatestRead)
            }
            _ => (tag.clone(), PonsCurveStateQuality::ExactBlockRead),
        };

        let reserves = self
            .eth_call_fn(&url, curve, GET_RESERVES, &tag_used)
            .await?;
        let words = decode_abi_words(&reserves);
        if words.len() < 2 {
            return Err(CurveReadError::new(
                CurveReadErrorKind::Invalid,
                "getReserves did not return two words",
            ));
        }
        let vq = words[0].clone();
        let vt = words[1].clone();
        if parse_u256(&vq).is_zero() || parse_u256(&vt).is_zero() {
            return Err(CurveReadError::new(
                CurveReadErrorKind::Invalid,
                "zero virtual reserve from getReserves",
            ));
        }

        let real_q = self
            .eth_call_fn(&url, curve, REAL_QUOTE_RESERVE, &tag_used)
            .await
            .ok()
            .and_then(|h| decode_abi_words(&h).into_iter().next())
            .unwrap_or_else(|| "0".into());
        let sellable = self
            .eth_call_fn(&url, curve, SELLABLE_TOKENS, &tag_used)
            .await
            .ok()
            .and_then(|h| decode_abi_words(&h).into_iter().next())
            .unwrap_or_else(|| "0".into());
        let threshold = self
            .eth_call_fn(&url, curve, GRADUATION_THRESHOLD, &tag_used)
            .await
            .ok()
            .and_then(|h| decode_abi_words(&h).into_iter().next())
            .unwrap_or_else(|| "0".into());
        let fee = self
            .eth_call_fn(&url, curve, FEE_BPS, &tag_used)
            .await
            .ok()
            .and_then(|h| decode_abi_words(&h).into_iter().next())
            .unwrap_or_else(|| "0".into());
        let ctax = self
            .eth_call_fn(&url, curve, CREATOR_TAX_BPS, &tag_used)
            .await
            .ok()
            .and_then(|h| decode_abi_words(&h).into_iter().next())
            .unwrap_or_else(|| "0".into());
        let graduated = self
            .eth_call_fn(&url, curve, GRADUATED, &tag_used)
            .await
            .ok()
            .map(|h| decode_abi_bool(&h))
            .unwrap_or(false);
        let ready = self
            .eth_call_fn(&url, curve, READY_TO_GRADUATE, &tag_used)
            .await
            .ok()
            .map(|h| decode_abi_bool(&h))
            .unwrap_or(false);

        let status = if graduated {
            PonsCurveStatus::Graduated
        } else if ready {
            PonsCurveStatus::ReadyToGraduate
        } else {
            PonsCurveStatus::Active
        };
        let fee_bps = u32::try_from(parse_u256(&fee)).unwrap_or(0).min(10_000);
        let creator_tax_bps = u32::try_from(parse_u256(&ctax)).unwrap_or(0).min(10_000);
        let progress = PonsCurveState::progress_from_reserves(&real_q, &threshold);

        let mut block_hash = None;
        if let Ok(v) = self
            .rpc(&url, "eth_getBlockByNumber", json!([tag, false]))
            .await
        {
            if let Some(h) = v.get("hash").and_then(|x| x.as_str()) {
                block_hash = Some(h.to_string());
            }
        }

        let quality = if tag_used == "latest" {
            PonsCurveStateQuality::LiveLatestRead
        } else {
            quality
        };

        let state = PonsCurveState {
            chain: Chain::Robinhood,
            token: token.to_ascii_lowercase(),
            curve: curve.to_string(),
            block_number: Some(block),
            block_hash,
            observed_at: Utc::now(),
            virtual_quote_reserve: vq,
            virtual_token_reserve: vt,
            real_quote_reserve: real_q.clone(),
            real_token_reserve: sellable,
            quote_collected: real_q,
            graduation_threshold: threshold,
            progress_bps: progress,
            status,
            fee_bps,
            creator_tax_bps,
            snipe_tax_bps: Some(9900),
            state_quality: quality,
            source: PONS_CURVE_SOURCE.into(),
            abi_version: PONS_CURVE_ABI_VERSION.into(),
        };

        if self.inner.historical.lock().expect("hist").is_none() {
            let cap = self.probe_historical_inner(&url, curve, block).await;
            *self.inner.historical.lock().expect("hist") = Some(cap);
        }
        Ok(state)
    }

    async fn probe_historical_inner(
        &self,
        url: &str,
        curve: &str,
        head: u64,
    ) -> HistoricalCallCapability {
        for off in [10u64, 100] {
            if head <= off {
                continue;
            }
            let b = head - off;
            let tag = format!("0x{b:x}");
            match self.eth_call_fn(url, curve, GET_RESERVES, &tag).await {
                Ok(_) => {}
                Err(e) => {
                    return HistoricalCallCapability {
                        supported: false,
                        tested_head: head,
                        tested_offsets: vec![0, off],
                        failed_offset: Some(off),
                        note: format!("historical eth_call failed at head-{off}: {}", e.message),
                    };
                }
            }
        }
        HistoricalCallCapability {
            supported: true,
            tested_head: head,
            tested_offsets: vec![0, 10, 100],
            failed_offset: None,
            note: "eth_call succeeded at latest, head-10, head-100".into(),
        }
    }

    pub async fn probe_historical(
        &self,
        curve: &str,
    ) -> std::result::Result<HistoricalCallCapability, CurveReadError> {
        if let Some(h) = self.historical_capability() {
            return Ok(h);
        }
        let head = self.head_block().await?;
        match &self.inner.mode {
            ReaderMode::Live(url) => {
                let cap = self.probe_historical_inner(url, curve, head).await;
                *self.inner.historical.lock().expect("hist") = Some(cap.clone());
                Ok(cap)
            }
            _ => Ok(HistoricalCallCapability {
                supported: true,
                tested_head: head,
                tested_offsets: vec![0, 10, 100],
                failed_offset: None,
                note: "non-live reader".into(),
            }),
        }
    }

    async fn eth_call_fn(
        &self,
        url: &str,
        to: &str,
        selector: &str,
        block_tag: &str,
    ) -> std::result::Result<String, CurveReadError> {
        self.eth_call_raw(url, to, selector, Some(block_tag)).await
    }

    async fn eth_call_raw(
        &self,
        url: &str,
        to: &str,
        data: &str,
        block_tag: Option<&str>,
    ) -> std::result::Result<String, CurveReadError> {
        let tag = block_tag.unwrap_or("latest");
        let params = json!([{"to": to, "data": data}, tag]);
        let mut last = CurveReadError::new(CurveReadErrorKind::Other, "no attempt");
        for attempt in 0..MAX_RETRIES {
            match self.rpc(url, "eth_call", params.clone()).await {
                Ok(Value::String(s)) => return Ok(s),
                Ok(other) => {
                    last = CurveReadError::new(
                        CurveReadErrorKind::Invalid,
                        format!("eth_call non-string {other}"),
                    );
                }
                Err(e) => {
                    last = classify_rpc(&e.to_string());
                    if matches!(
                        last.kind,
                        CurveReadErrorKind::Timeout | CurveReadErrorKind::RateLimit
                    ) {
                        let backoff = Duration::from_millis(200u64.saturating_mul(1u64 << attempt));
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    return Err(last);
                }
            }
        }
        Err(last)
    }

    async fn rpc(&self, url: &str, method: &str, params: Value) -> Result<Value> {
        http_jsonrpc(&self.inner.http, url, method, params).await
    }

    fn note_failure(&self, kind: CurveReadErrorKind) {
        self.inner.failures.fetch_add(1, Ordering::Relaxed);
        DiscoveryMetrics::pons_curve_state_read_failure(kind.as_str());
        if kind == CurveReadErrorKind::RateLimit {
            self.inner.rate_limits.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn classify_rpc(err: &str) -> CurveReadError {
    let s = err.to_ascii_lowercase();
    if s.contains("timeout") || s.contains("timed out") || s.contains("deadline") {
        return CurveReadError::new(CurveReadErrorKind::Timeout, err);
    }
    if s.contains("429")
        || s.contains("rate limit")
        || s.contains("too many requests")
        || s.contains("-32005")
        || s.contains("over rate")
    {
        return CurveReadError::new(CurveReadErrorKind::RateLimit, err);
    }
    if s.contains("no code") || s.contains("not found") {
        return CurveReadError::new(CurveReadErrorKind::NotFound, err);
    }
    CurveReadError::new(CurveReadErrorKind::Other, err)
}

pub fn classify_paper_failure(
    reason: Option<&str>,
    status: crate::sim::types::ExecutionStatus,
) -> &'static str {
    use crate::sim::types::ExecutionStatus;
    let r = reason.unwrap_or("").to_ascii_uppercase();
    if r.contains("PROVIDER_TIMEOUT") {
        return "PROVIDER_TIMEOUT";
    }
    if r.contains("PROVIDER_RATE_LIMIT") || r.contains("RATE_LIMIT") || r.contains("429") {
        return "PROVIDER_RATE_LIMIT";
    }
    if r.contains("CURVE_NOT_FOUND") {
        return "CURVE_NOT_FOUND";
    }
    if r.contains("INVALID_CURVE")
        || r.contains("ZERO_VIRTUAL")
        || r.contains("UNKNOWN_CURVE_QUALITY")
    {
        return "INVALID_CURVE_STATE";
    }
    if r.contains("GRADUATION_GAP") {
        return "GRADUATION_GAP";
    }
    if r.contains("INSUFFICIENT")
        || r.contains("ZERO_TOKEN")
        || r.contains("ZERO_QUOTE")
        || r.contains("MARKET_NOT_SELLABLE")
        || matches!(status, ExecutionStatus::RejectedLiquidity)
    {
        return "INSUFFICIENT_LIQUIDITY";
    }
    if r.contains("SLIPPAGE")
        || r.contains("IMPACT_")
        || r.contains("FEE_TAX_CONSUMED")
        || matches!(status, ExecutionStatus::RejectedSlippage)
    {
        return "SLIPPAGE_LIMIT";
    }
    if r.contains("SEEDED_FAILURE") || r.contains("EXECUTION_FAILURE") {
        return "EXECUTION_FAILURE_MODEL";
    }
    if r.contains("UNKNOWN_CURVE_RESERVES") {
        return "UNKNOWN_CURVE_RESERVES";
    }
    if r.contains("TIMEOUT") {
        return "PROVIDER_TIMEOUT";
    }
    "OTHER"
}

pub fn execution_quality_label(curve_q: PonsCurveStateQuality, filled: bool) -> &'static str {
    if filled && curve_q.research_valid_live_paper() {
        "MODELLED_HIGH_CONFIDENCE"
    } else if filled {
        "MODELLED"
    } else {
        "NON_RESEARCH_VALID"
    }
}

pub fn _keep_u256_dec(v: alloy_primitives::U256) -> String {
    u256_dec(v)
}
