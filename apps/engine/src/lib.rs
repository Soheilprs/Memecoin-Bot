pub mod artifacts;
pub mod candidate;
pub mod collect;
pub mod config;
pub mod decoders;
pub mod domain;
pub mod error;
pub mod features;
pub mod historical;
pub mod ingest;
pub mod lab;
pub mod live;
pub mod metrics;
pub mod normalize;
pub mod pipeline;
pub mod prospective;
pub mod registry;
pub mod replay;
pub mod security;
pub mod sim;
pub mod state;
pub mod storage;
pub mod strategy;
pub mod test_support;
pub mod wallet;
pub mod watch;

pub use domain::{
    validate_dataset_quality, CollectionSession, LifecycleObserved, QualityCheck, QualityStatus,
    RawEvent, SolanaMode, TokenDiscovered, TradeObserved,
};
pub use pipeline::DiscoveryPipeline;
