use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("malformed event: {0}")]
    Malformed(String),
    #[error("unknown decoder version {requested} for {protocol} (pinned {pinned})")]
    DecoderVersionMismatch {
        protocol: String,
        requested: String,
        pinned: String,
    },
    #[error("decoder mismatch: {0}")]
    DecoderMismatch(String),
    #[error("empty bytecode")]
    EmptyBytecode,
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("unknown abi: {0}")]
    UnknownAbi(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("ingest: {0}")]
    Ingest(String),
    #[error(transparent)]
    DatasetQuality(#[from] DatasetQualityError),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DatasetQualityError {
    #[error("incomplete source: chain={chain} mode={mode} status={status}")]
    IncompleteSource {
        chain: String,
        mode: String,
        status: String,
    },
}

pub type Result<T> = std::result::Result<T, EngineError>;
