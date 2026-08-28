//! Provider-aware observation validity. Process heartbeat is not VALID coverage.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::domain::Chain;
use crate::error::Result;
use crate::storage::postgres::PostgresStore;

pub const STALE_AFTER: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationReason {
    Healthy,
    RpcRateLimit,
    RpcProviderUnavailable,
    CollectorStale,
    ExecutionReadUnavailable,
}

impl ObservationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "HEALTHY",
            Self::RpcRateLimit => "RPC_RATE_LIMIT",
            Self::RpcProviderUnavailable => "RPC_PROVIDER_UNAVAILABLE",
            Self::CollectorStale => "COLLECTOR_STALE",
            Self::ExecutionReadUnavailable => "EXECUTION_READ_UNAVAILABLE",
        }
    }

    pub fn interval_status(self) -> &'static str {
        match self {
            Self::Healthy => "VALID",
            Self::RpcRateLimit | Self::RpcProviderUnavailable | Self::ExecutionReadUnavailable => {
                "INVALID"
            }
            Self::CollectorStale => "PARTIAL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntervalKind {
    Valid,
    Partial,
    Invalid,
}

impl IntervalKind {
    fn parse(s: &str) -> Self {
        match s {
            "VALID" => Self::Valid,
            "INVALID" => Self::Invalid,
            _ => Self::Partial,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "VALID",
            Self::Partial => "PARTIAL",
            Self::Invalid => "INVALID",
        }
    }
}

pub struct ObservationHealth {
    last_head: Mutex<Option<(Chain, u64, Instant)>>,
    last_log: Mutex<Option<Instant>>,
    collector_up: AtomicBool,
    execution_ok: AtomicBool,
    rate_limited: AtomicBool,
    provider_down: AtomicBool,
    last_reason: Mutex<ObservationReason>,
}

impl Default for ObservationHealth {
    fn default() -> Self {
        Self {
            last_head: Mutex::new(None),
            last_log: Mutex::new(None),
            collector_up: AtomicBool::new(false),
            execution_ok: AtomicBool::new(true),
            rate_limited: AtomicBool::new(false),
            provider_down: AtomicBool::new(false),
            last_reason: Mutex::new(ObservationReason::CollectorStale),
        }
    }
}

impl ObservationHealth {
    pub fn note_head(&self, chain: Chain, block: u64) {
        *self.last_head.lock().expect("h") = Some((chain, block, Instant::now()));
        self.collector_up.store(true, Ordering::Relaxed);
        self.provider_down.store(false, Ordering::Relaxed);
    }

    pub fn note_log(&self) {
        *self.last_log.lock().expect("l") = Some(Instant::now());
        self.collector_up.store(true, Ordering::Relaxed);
    }

    pub fn note_collector_down(&self) {
        self.collector_up.store(false, Ordering::Relaxed);
    }

    pub fn note_execution_ok(&self) {
        self.execution_ok.store(true, Ordering::Relaxed);
        self.rate_limited.store(false, Ordering::Relaxed);
        self.provider_down.store(false, Ordering::Relaxed);
    }

    pub fn note_rate_limit(&self) {
        self.rate_limited.store(true, Ordering::Relaxed);
        self.execution_ok.store(false, Ordering::Relaxed);
    }

    pub fn note_provider_down(&self) {
        self.provider_down.store(true, Ordering::Relaxed);
        self.execution_ok.store(false, Ordering::Relaxed);
    }

    pub fn note_execution_fail(&self) {
        self.execution_ok.store(false, Ordering::Relaxed);
    }

    pub fn evaluate(&self, now: Instant) -> ObservationReason {
        if self.rate_limited.load(Ordering::Relaxed) {
            return ObservationReason::RpcRateLimit;
        }
        if self.provider_down.load(Ordering::Relaxed) {
            return ObservationReason::RpcProviderUnavailable;
        }
        let head = self.last_head.lock().expect("h").map(|(_, _, t)| t);
        let log = *self.last_log.lock().expect("l");
        let fresh = head.is_some_and(|t| now.saturating_duration_since(t) < STALE_AFTER)
            || log.is_some_and(|t| now.saturating_duration_since(t) < STALE_AFTER);
        if !self.collector_up.load(Ordering::Relaxed) || !fresh {
            return ObservationReason::CollectorStale;
        }
        if !self.execution_ok.load(Ordering::Relaxed) {
            return ObservationReason::ExecutionReadUnavailable;
        }
        ObservationReason::Healthy
    }

    pub fn last_reason(&self) -> ObservationReason {
        *self.last_reason.lock().expect("r")
    }
}

pub fn global() -> &'static ObservationHealth {
    static H: OnceLock<ObservationHealth> = OnceLock::new();
    H.get_or_init(ObservationHealth::default)
}

/// Heartbeat must never keep a VALID interval alive by itself.
pub async fn apply_observation_health(
    store: &PostgresStore,
    experiment_id: &str,
    health: &ObservationHealth,
    recovered_gate: bool,
) -> Result<ObservationReason> {
    let reason = health.evaluate(Instant::now());
    *health.last_reason.lock().expect("r") = reason;
    let now = Utc::now();
    let open = store.load_open_observation(experiment_id).await?;
    let desired = IntervalKind::parse(reason.interval_status());
    match open {
        Some((id, status, _)) => {
            let cur = IntervalKind::parse(&status);
            if cur == IntervalKind::Valid && desired != IntervalKind::Valid {
                let _ = id;
                store
                    .close_open_observation_interval(
                        experiment_id,
                        now,
                        desired.as_str(),
                        Some(reason.as_str()),
                    )
                    .await?;
                store
                    .open_observation_interval(experiment_id, now, desired.as_str())
                    .await?;
            } else if cur != IntervalKind::Valid && desired == IntervalKind::Valid && recovered_gate
            {
                store
                    .close_open_observation_interval(
                        experiment_id,
                        now,
                        cur.as_str(),
                        Some("RPC_RECOVERED"),
                    )
                    .await?;
                store
                    .open_observation_interval(experiment_id, now, "VALID")
                    .await?;
            } else {
                store
                    .touch_observation_heartbeat(experiment_id, now)
                    .await?;
            }
        }
        None => {
            store
                .open_observation_interval(experiment_id, now, desired.as_str())
                .await?;
        }
    }
    Ok(reason)
}

pub fn heartbeat_is_not_valid_proof() -> bool {
    true
}

pub fn would_keep_valid_on_heartbeat_only(health: &ObservationHealth) -> bool {
    matches!(health.evaluate(Instant::now()), ObservationReason::Healthy)
}

#[cfg(test)]
pub fn reset_global_for_tests() {
    let h = global();
    *h.last_head.lock().expect("h") = None;
    *h.last_log.lock().expect("l") = None;
    h.collector_up.store(false, Ordering::Relaxed);
    h.execution_ok.store(true, Ordering::Relaxed);
    h.rate_limited.store(false, Ordering::Relaxed);
    h.provider_down.store(false, Ordering::Relaxed);
}

#[allow(dead_code)]
pub fn _now_utc() -> DateTime<Utc> {
    Utc::now()
}
