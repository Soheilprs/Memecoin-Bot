//! RPC request accounting. Never stores URLs or API keys.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RpcPurpose {
    PonsCurve,
    Collector,
    Backfill,
    Head,
    Security,
    PaperEntry,
    PaperExit,
    Preflight,
    BaseShadow,
}

impl RpcPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PonsCurve => "PONS_CURVE",
            Self::Collector => "COLLECTOR",
            Self::Backfill => "BACKFILL",
            Self::Head => "HEAD",
            Self::Security => "SECURITY",
            Self::PaperEntry => "PAPER_ENTRY",
            Self::PaperExit => "PAPER_EXIT",
            Self::Preflight => "PREFLIGHT",
            Self::BaseShadow => "BASE_SHADOW",
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RpcSnapshot {
    pub elapsed_secs: u64,
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub by_method: BTreeMap<String, u64>,
    pub by_purpose: BTreeMap<String, u64>,
    pub by_chain: BTreeMap<String, u64>,
    pub fail_by_method: BTreeMap<String, u64>,
    pub per_minute: f64,
}

struct RpcAcc {
    started: Instant,
    total: AtomicU64,
    success: AtomicU64,
    failure: AtomicU64,
    by_method: Mutex<BTreeMap<String, u64>>,
    by_purpose: Mutex<BTreeMap<String, u64>>,
    by_chain: Mutex<BTreeMap<String, u64>>,
    fail_by_method: Mutex<BTreeMap<String, u64>>,
}

fn acc() -> &'static RpcAcc {
    static A: OnceLock<RpcAcc> = OnceLock::new();
    A.get_or_init(|| RpcAcc {
        started: Instant::now(),
        total: AtomicU64::new(0),
        success: AtomicU64::new(0),
        failure: AtomicU64::new(0),
        by_method: Mutex::new(BTreeMap::new()),
        by_purpose: Mutex::new(BTreeMap::new()),
        by_chain: Mutex::new(BTreeMap::new()),
        fail_by_method: Mutex::new(BTreeMap::new()),
    })
}

fn bump(map: &Mutex<BTreeMap<String, u64>>, k: &str) {
    let mut g = map.lock().expect("rpc map");
    *g.entry(k.to_string()).or_insert(0) += 1;
}

pub fn record(
    chain: &str,
    method: &str,
    purpose: RpcPurpose,
    ok: bool,
    _latency: Duration,
    _status: Option<u16>,
) {
    let a = acc();
    a.total.fetch_add(1, Ordering::Relaxed);
    if ok {
        a.success.fetch_add(1, Ordering::Relaxed);
    } else {
        a.failure.fetch_add(1, Ordering::Relaxed);
        bump(&a.fail_by_method, method);
    }
    bump(&a.by_method, method);
    bump(&a.by_purpose, purpose.as_str());
    bump(&a.by_chain, chain);
}

pub fn snapshot() -> RpcSnapshot {
    let a = acc();
    let elapsed = a.started.elapsed().as_secs().max(1);
    let total = a.total.load(Ordering::Relaxed);
    RpcSnapshot {
        elapsed_secs: elapsed,
        total,
        success: a.success.load(Ordering::Relaxed),
        failure: a.failure.load(Ordering::Relaxed),
        by_method: a.by_method.lock().expect("m").clone(),
        by_purpose: a.by_purpose.lock().expect("p").clone(),
        by_chain: a.by_chain.lock().expect("c").clone(),
        fail_by_method: a.fail_by_method.lock().expect("f").clone(),
        per_minute: total as f64 * 60.0 / elapsed as f64,
    }
}

#[cfg(test)]
pub fn reset_for_tests() {
    let a = acc();
    a.total.store(0, Ordering::Relaxed);
    a.success.store(0, Ordering::Relaxed);
    a.failure.store(0, Ordering::Relaxed);
    a.by_method.lock().expect("m").clear();
    a.by_purpose.lock().expect("p").clear();
    a.by_chain.lock().expect("c").clear();
    a.fail_by_method.lock().expect("f").clear();
}
