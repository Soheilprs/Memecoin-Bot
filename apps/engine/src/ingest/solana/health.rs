use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::Utc;

use crate::domain::Chain;
use crate::metrics::DiscoveryMetrics;
use crate::storage::{EventStore, IngestGap};

const NONE: u64 = 0;

#[derive(Default)]
pub struct SolanaSlotTracker {
    head: AtomicU64,
    received: AtomicU64,
    persisted: AtomicU64,
    confirmed: AtomicU64,
    finalized: AtomicU64,
    last_slot_seen: AtomicU64,
    missing_ranges: Mutex<Vec<(u64, u64)>>,
}

impl SolanaSlotTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_head(&self, slot: u64) {
        fetch_max(&self.head, slot);
        self.detect_slot_skip(slot);
        self.last_slot_seen.store(slot, Ordering::Relaxed);
        self.publish();
    }

    pub fn note_confirmed(&self, slot: u64) {
        fetch_max(&self.confirmed, slot);
        self.publish();
    }

    pub fn note_finalized(&self, slot: u64) {
        fetch_max(&self.finalized, slot);
        self.publish();
    }

    pub fn note_received(&self, slot: u64) {
        fetch_max(&self.received, slot);
        self.publish();
    }

    pub fn note_persisted(&self, slot: u64) {
        fetch_max(&self.persisted, slot);
        self.publish();
    }

    pub fn head(&self) -> u64 {
        self.head.load(Ordering::Relaxed)
    }
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }
    pub fn persisted(&self) -> u64 {
        self.persisted.load(Ordering::Relaxed)
    }
    pub fn confirmed(&self) -> u64 {
        self.confirmed.load(Ordering::Relaxed)
    }
    pub fn finalized(&self) -> u64 {
        self.finalized.load(Ordering::Relaxed)
    }

    pub fn slot_lag(&self) -> u64 {
        self.head().saturating_sub(self.received())
    }

    pub fn persist_lag(&self) -> u64 {
        self.received().saturating_sub(self.persisted())
    }

    fn detect_slot_skip(&self, slot: u64) {
        let prev = self.last_slot_seen.load(Ordering::Relaxed);
        if prev == NONE || slot <= prev + 1 {
            return;
        }
        let from = prev + 1;
        let to = slot - 1;
        if let Ok(mut g) = self.missing_ranges.lock() {
            g.push((from, to));
        }
        DiscoveryMetrics::solana_missing_slot_range();
    }

    pub fn take_missing_ranges(&self) -> Vec<(u64, u64)> {
        self.missing_ranges
            .lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default()
    }

    pub fn publish(&self) {
        DiscoveryMetrics::solana_slots(
            self.head(),
            self.received(),
            self.persisted(),
            self.finalized(),
            self.slot_lag(),
        );
    }

    pub async fn flush_gaps<S: EventStore>(&self, store: &S, reason: &str) {
        for (from, to) in self.take_missing_ranges() {
            let gap = IngestGap {
                id: None,
                chain: Chain::Solana,
                source: "yellowstone".into(),
                stream: "solana:pumpfun".into(),
                from_block: None,
                to_block: None,
                from_slot: Some(from as i64),
                to_slot: Some(to as i64),
                detected_at: Utc::now(),
                recovered: false,
                recovered_at: None,
                reason: reason.into(),
            };
            let _ = store.insert_gap(&gap).await;
            DiscoveryMetrics.stream_gap(Chain::Solana);
        }
    }
}

fn fetch_max(slot: &AtomicU64, v: u64) {
    let mut cur = slot.load(Ordering::Relaxed);
    while v > cur {
        match slot.compare_exchange_weak(cur, v, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

pub async fn record_missing_range<S: EventStore>(
    store: &S,
    from_slot: u64,
    to_slot: u64,
    reason: &str,
) -> crate::error::Result<i64> {
    let gap = IngestGap {
        id: None,
        chain: Chain::Solana,
        source: "yellowstone".into(),
        stream: "solana:pumpfun".into(),
        from_block: None,
        to_block: None,
        from_slot: Some(from_slot as i64),
        to_slot: Some(to_slot as i64),
        detected_at: Utc::now(),
        recovered: false,
        recovered_at: None,
        reason: reason.into(),
    };
    store.insert_gap(&gap).await
}
