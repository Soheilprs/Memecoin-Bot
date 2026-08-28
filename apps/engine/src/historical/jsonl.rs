use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};

use crate::domain::{Finality, RawEvent};
use crate::error::{EngineError, Result};
use crate::ingest::solana::parse::raw_events_from_get_transaction;

use super::HistoricalSource;

/// Line-oriented file source. One JSON object per line; does not load the file into RAM.
///
/// Intended for future Pump.fun corpus import (launches, trades, snapshots,
/// wallet activity, graduations) as newline-delimited JSON. Process in batches
/// by calling [`HistoricalSource::next_event`] in a loop.
pub struct JsonlSource {
    lines: std::io::Lines<BufReader<File>>,
    pending: VecDeque<RawEvent>,
    source: String,
}

impl JsonlSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| EngineError::Ingest(format!("open {}: {e}", path.display())))?;
        Ok(Self {
            lines: BufReader::new(file).lines(),
            pending: VecDeque::new(),
            source: "historical:jsonl".into(),
        })
    }

    fn parse_line(&self, line: &str) -> Result<Vec<RawEvent>> {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| EngineError::Ingest(format!("jsonl: {e}")))?;
        if let Ok(mut raw) = serde_json::from_value::<RawEvent>(v.clone()) {
            if raw.source.is_empty() {
                raw.source = self.source.clone();
            }
            return Ok(vec![raw]);
        }
        if let Ok(record) = serde_json::from_value::<crate::domain::CorpusRecord>(v.clone()) {
            return Ok(vec![super::corpus::raw_from_record(record, &self.source)]);
        }
        let observed = v
            .get("block_time")
            .and_then(serde_json::Value::as_i64)
            .and_then(|t| Utc.timestamp_opt(t, 0).single())
            .unwrap_or_else(|| Utc.timestamp_opt(1_700_000_000, 0).single().unwrap());
        Ok(raw_events_from_get_transaction(
            &v,
            &self.source,
            observed,
            Finality::Finalized,
        ))
    }
}

#[async_trait]
impl HistoricalSource for JsonlSource {
    async fn next_event(&mut self) -> Result<Option<RawEvent>> {
        if let Some(raw) = self.pending.pop_front() {
            return Ok(Some(raw));
        }
        loop {
            let line = match self.lines.next() {
                None => return Ok(None),
                Some(Ok(line)) => line,
                Some(Err(e)) => return Err(EngineError::Ingest(format!("jsonl read: {e}"))),
            };
            if line.trim().is_empty() {
                continue;
            }
            let mut events = self.parse_line(&line)?;
            if events.is_empty() {
                continue;
            }
            let first = events.remove(0);
            self.pending.extend(events);
            return Ok(Some(first));
        }
    }
}
