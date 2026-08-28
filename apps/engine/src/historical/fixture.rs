use std::collections::{HashMap, VecDeque};
use std::path::Path;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};

use crate::domain::{Finality, RawEvent};
use crate::error::{EngineError, Result};
use crate::ingest::solana::parse::raw_events_from_get_transaction;

use super::HistoricalSource;

/// Offline fixture directory. Small by design; corpus import uses [`super::JsonlSource`].
pub struct FixtureSource {
    events: VecDeque<RawEvent>,
}

impl FixtureSource {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(EngineError::Ingest(format!(
                "fixture dir not found: {}",
                dir.display()
            )));
        }
        let mut by_id: HashMap<String, RawEvent> = HashMap::new();
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| EngineError::Ingest(format!("read {}: {e}", dir.display())))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        files.sort();
        for path in files {
            for raw in load_raw_events(&path)? {
                by_id.insert(raw.event_id(), raw);
            }
        }
        let mut events: Vec<_> = by_id.into_values().collect();
        events.sort_by_key(replay_sort_key);
        Ok(Self {
            events: events.into(),
        })
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[async_trait]
impl HistoricalSource for FixtureSource {
    async fn next_event(&mut self) -> Result<Option<RawEvent>> {
        Ok(self.events.pop_front())
    }
}

pub fn replay_sort_key(raw: &RawEvent) -> (u64, String, u64, u32, u32) {
    (
        raw.slot().unwrap_or(0) as u64,
        raw.tx_hash().to_string(),
        raw.transaction_index().unwrap_or(0) as u64,
        raw.instruction_index().unwrap_or(0) as u32,
        raw.inner_instruction_index()
            .map(|v| v.saturating_add(1) as u32)
            .unwrap_or(0),
    )
}

pub fn load_raw_events(path: &Path) -> Result<Vec<RawEvent>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| EngineError::Ingest(format!("read {}: {e}", path.display())))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| EngineError::Ingest(format!("json {}: {e}", path.display())))?;
    let observed = v
        .get("block_time")
        .and_then(serde_json::Value::as_i64)
        .and_then(|t| Utc.timestamp_opt(t, 0).single())
        .unwrap_or_else(|| Utc.timestamp_opt(1_700_000_000, 0).single().unwrap());
    if v.get("transaction").is_some() {
        return Ok(raw_events_from_get_transaction(
            &v,
            "historical:fixture",
            observed,
            Finality::Finalized,
        ));
    }
    if let Ok(mut raw) = serde_json::from_value::<RawEvent>(v) {
        raw.source = "historical:fixture".into();
        raw.observed_at = observed;
        return Ok(vec![raw]);
    }
    Err(EngineError::Ingest(format!(
        "unsupported fixture {}",
        path.display()
    )))
}
