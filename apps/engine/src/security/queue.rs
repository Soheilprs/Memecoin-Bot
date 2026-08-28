use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::error::Result;
use crate::metrics::DiscoveryMetrics;
use crate::security::assessment::SecurityAssessment;
use crate::security::context::SecurityContext;
use crate::security::engine::SecurityEngine;
use crate::storage::EventStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityPriority {
    Scheduled = 4,
    Lifecycle = 3,
    PreEntry = 2,
    Discovered = 1,
}

pub struct SecurityJob {
    pub priority: SecurityPriority,
    pub ctx: SecurityContext,
}

pub struct SecurityWorkQueue {
    tx: Sender<SecurityJob>,
}

impl SecurityWorkQueue {
    pub fn bounded(cap: usize) -> (Self, Receiver<SecurityJob>) {
        let (tx, rx) = channel(cap);
        (Self { tx }, rx)
    }

    /// Never silently drop. On saturation, wait; caller may persist UNKNOWN timeout.
    pub async fn submit(&self, job: SecurityJob) -> Result<()> {
        DiscoveryMetrics::security_queue_depth_inc();
        match self.tx.try_send(job) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(job)) => {
                DiscoveryMetrics::security_queue_saturated();
                tokio::time::timeout(Duration::from_secs(5), self.tx.send(job))
                    .await
                    .map_err(|_| {
                        crate::error::EngineError::Ingest("security queue timeout".into())
                    })?
                    .map_err(|e| crate::error::EngineError::Ingest(format!("security queue: {e}")))
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => Err(
                crate::error::EngineError::Ingest("security queue closed".into()),
            ),
        }
    }
}

pub async fn run_worker<S: EventStore>(
    mut rx: Receiver<SecurityJob>,
    engine: Arc<SecurityEngine>,
    store: Arc<S>,
) {
    while let Some(job) = rx.recv().await {
        DiscoveryMetrics::security_queue_depth_dec();
        let a = engine.assess(&job.ctx);
        let _ = persist_assessment(store.as_ref(), &a).await;
    }
}

pub async fn persist_assessment<S: EventStore>(store: &S, a: &SecurityAssessment) -> Result<i64> {
    store.insert_assessment(a).await
}
