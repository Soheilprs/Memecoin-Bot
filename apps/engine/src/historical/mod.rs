//! Historical Solana sources. Stream/batch; never load a corpus into RAM.

use async_trait::async_trait;

use crate::domain::RawEvent;
use crate::error::Result;

pub mod corpus;
pub mod fixture;
pub mod jsonl;
pub mod manifest;
pub mod validate;

pub use corpus::PumpCorpusSource;
pub use fixture::FixtureSource;
pub use jsonl::JsonlSource;
pub use manifest::{sha256_bytes, sha256_file, DatasetManifest, FileChecksum};
pub use validate::{
    detect_hour_gaps, exp001_historical_security_policy, graduation_bias, scan_raw_events,
    validate_historical_dataset, DatasetValidation, DatasetVerdict, GraduationBias, StreamingScan,
};

/// FixtureSource, JsonlSource, PumpCorpusSource. Parquet is converted to JSONL by the importer.
#[async_trait]
pub trait HistoricalSource: Send {
    async fn next_event(&mut self) -> Result<Option<RawEvent>>;
}
