//! Streaming decoded Pump.fun corpus source. Never loads the full file into RAM.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use async_trait::async_trait;

use crate::domain::{
    CanonicalStatus, CorpusRecord, DecoderStatus, Finality, RawEvent, RawEventKind,
};
use crate::error::{EngineError, Result};

use super::HistoricalSource;

pub const CORPUS_SOURCE_LABEL: &str = "historical:pumpfun_corpus";

pub struct PumpCorpusSource {
    lines: std::io::Lines<BufReader<File>>,
    pending: VecDeque<RawEvent>,
    source: String,
}

impl PumpCorpusSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| EngineError::Ingest(format!("open {}: {e}", path.display())))?;
        Ok(Self {
            lines: BufReader::new(file).lines(),
            pending: VecDeque::new(),
            source: CORPUS_SOURCE_LABEL.into(),
        })
    }

    fn parse_line(&self, line: &str) -> Result<Option<RawEvent>> {
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| EngineError::Ingest(format!("corpus jsonl: {e}")))?;
        if let Ok(mut raw) = serde_json::from_value::<RawEvent>(v.clone()) {
            if raw.source.is_empty() {
                raw.source = self.source.clone();
            }
            if raw.as_corpus().is_none() && raw.as_solana().is_none() && raw.as_evm().is_none() {
                return Ok(None);
            }
            return Ok(Some(raw));
        }
        if let Ok(record) = serde_json::from_value::<CorpusRecord>(v) {
            return Ok(Some(raw_from_record(record, &self.source)));
        }
        Err(EngineError::Ingest(
            "corpus jsonl line is neither RawEvent nor CorpusRecord".into(),
        ))
    }
}

pub fn raw_from_record(record: CorpusRecord, source: &str) -> RawEvent {
    let observed = record.timestamp;
    RawEvent {
        kind: RawEventKind::DecodedCorpus(Box::new(record)),
        source: source.to_string(),
        observed_at: observed,
        persisted_at: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Finalized,
        decoder_status: DecoderStatus::Pending,
        decoder_version: Some(crate::domain::NORMALIZATION_VERSION.into()),
        error: None,
    }
}

#[async_trait]
impl HistoricalSource for PumpCorpusSource {
    async fn next_event(&mut self) -> Result<Option<RawEvent>> {
        if let Some(raw) = self.pending.pop_front() {
            return Ok(Some(raw));
        }
        loop {
            let line = match self.lines.next() {
                None => return Ok(None),
                Some(Ok(line)) => line,
                Some(Err(e)) => return Err(EngineError::Ingest(format!("corpus jsonl read: {e}"))),
            };
            if line.trim().is_empty() {
                continue;
            }
            match self.parse_line(&line)? {
                Some(raw) => return Ok(Some(raw)),
                None => continue,
            }
        }
    }
}
