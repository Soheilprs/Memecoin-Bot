//! Dataset manifest + deterministic hash. Large parquet files stay outside Git.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::{IMPORTER_VERSION, SLKY_DATASET_ID, SLKY_SOURCE_URL};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChecksum {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetManifest {
    pub dataset_name: String,
    pub source: String,
    pub source_url: String,
    pub publisher: String,
    #[serde(default)]
    pub license: Option<String>,
    pub retrieved_at: String,
    pub original_files: Vec<FileChecksum>,
    #[serde(default)]
    pub declared_period_start: Option<String>,
    #[serde(default)]
    pub declared_period_end: Option<String>,
    #[serde(default)]
    pub observed_period_start: Option<String>,
    #[serde(default)]
    pub observed_period_end: Option<String>,
    #[serde(default)]
    pub raw_row_counts: serde_json::Value,
    #[serde(default)]
    pub token_count: Option<u64>,
    #[serde(default)]
    pub trade_count: Option<u64>,
    #[serde(default)]
    pub graduation_count: Option<u64>,
    pub format: String,
    pub schema_version: String,
    pub importer_version: String,
    #[serde(default)]
    pub known_limitations: Vec<String>,
    #[serde(default)]
    pub dataset_hash: Option<String>,
}

impl DatasetManifest {
    pub fn slinky21_template(retrieved_at: impl Into<String>) -> Self {
        Self {
            dataset_name: SLKY_DATASET_ID.into(),
            source: "huggingface".into(),
            source_url: SLKY_SOURCE_URL.into(),
            publisher: "Slink Dev (slink21taken)".into(),
            license: Some("CC BY 4.0 (README); Hugging Face card also lists MIT".into()),
            retrieved_at: retrieved_at.into(),
            original_files: Vec::new(),
            declared_period_start: Some("2026-06-05".into()),
            declared_period_end: Some("2026-07-14".into()),
            observed_period_start: None,
            observed_period_end: None,
            raw_row_counts: serde_json::json!({}),
            token_count: Some(798_430),
            trade_count: Some(33_581_765),
            graduation_count: Some(5_689),
            format: "parquet".into(),
            schema_version: "slinky21-2026-07".into(),
            importer_version: IMPORTER_VERSION.into(),
            known_limitations: vec![
                "DECODED_RESEARCH_CORPUS: not raw Solana transactions".into(),
                "No signatures/slots/instruction indices in published tables".into(),
                "sol_amount NULL ~7.03%; inconsistent ~3.38%".into(),
                "Jul 3 2026 websocket outage (zero trades)".into(),
                "Do not use snapshots.parquet heartbeat carry-forwards for fills".into(),
                "Do not use entry_price_*_usd (graduation leak)".into(),
                "wallet_stats activity columns are stale".into(),
            ],
            dataset_hash: None,
        }
    }

    pub fn canonical_hash_payload(&self) -> String {
        let mut files: Vec<_> = self
            .original_files
            .iter()
            .map(|f| format!("{}:{}:{}", f.path, f.sha256, f.size_bytes))
            .collect();
        files.sort();
        format!(
            "importer={}\nschema={}\nfiles=\n{}",
            self.importer_version,
            self.schema_version,
            files.join("\n")
        )
    }

    pub fn compute_dataset_hash(&self) -> String {
        hex::encode(Sha256::digest(self.canonical_hash_payload().as_bytes()))
    }

    pub fn with_hash(mut self) -> Self {
        self.dataset_hash = Some(self.compute_dataset_hash());
        self
    }
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn sha256_file(path: &std::path::Path) -> std::io::Result<FileChecksum> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok(FileChecksum {
        path: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        size_bytes: size,
        sha256: hex::encode(hasher.finalize()),
    })
}
