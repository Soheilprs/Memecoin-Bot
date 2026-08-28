use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::candidate::CandidateTransition;
use crate::domain::{
    CanonicalStatus, Chain, CollectionSession, DecoderStatus, Finality, LifecycleObserved,
    QualityStatus, RawEvent, TokenDiscovered, TradeObserved,
};
use crate::error::Result;
use crate::features::FeatureVector;
use crate::security::SecurityAssessment;
use crate::state::TokenStateSnapshot;
use crate::watch::MarketRef;

pub mod dbcheck;
pub mod memory;
pub mod postgres;
pub mod repositories;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertRaw {
    Inserted,
    Duplicate,
}

#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub ingest_id: String,
    pub chain: crate::domain::Chain,
    pub stream: String,
    pub last_block: Option<i64>,
    pub last_block_hash: Option<String>,
    pub last_finalized_block: Option<i64>,
    pub last_slot: Option<i64>,
    pub last_confirmed_slot: Option<i64>,
    pub last_finalized_slot: Option<i64>,
    pub last_signature: Option<String>,
    pub overlap_blocks: i32,
    pub overlap_slots: i32,
}

impl Checkpoint {
    pub fn new(ingest_id: impl Into<String>, chain: Chain) -> Self {
        Self {
            ingest_id: ingest_id.into(),
            chain,
            stream: "default".into(),
            last_block: None,
            last_block_hash: None,
            last_finalized_block: None,
            last_slot: None,
            last_confirmed_slot: None,
            last_finalized_slot: None,
            last_signature: None,
            overlap_blocks: 64,
            overlap_slots: 32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IngestGap {
    pub id: Option<i64>,
    pub chain: Chain,
    pub source: String,
    pub stream: String,
    pub from_block: Option<i64>,
    pub to_block: Option<i64>,
    pub from_slot: Option<i64>,
    pub to_slot: Option<i64>,
    pub detected_at: DateTime<Utc>,
    pub recovered: bool,
    pub recovered_at: Option<DateTime<Utc>>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ChainHead {
    pub chain: Chain,
    pub latest_block: Option<i64>,
    pub latest_block_hash: Option<String>,
    pub latest_slot: Option<i64>,
    pub finalized_block: Option<i64>,
    pub finalized_slot: Option<i64>,
    pub observed_at: DateTime<Utc>,
    pub lag_ms: Option<i64>,
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait EventStore: Send + Sync {
    async fn insert_raw(&self, event: &RawEvent) -> Result<InsertRaw>;
    async fn insert_discovered(&self, token: &TokenDiscovered) -> Result<()>;
    async fn insert_trade(&self, trade: &TradeObserved) -> Result<()>;
    async fn insert_lifecycle(&self, life: &LifecycleObserved) -> Result<()>;
    async fn mark_decoder(
        &self,
        event_id: &str,
        status: DecoderStatus,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<()>;
    async fn mark_orphaned(&self, event_id: &str) -> Result<bool>;
    async fn get_raw(&self, event_id: &str) -> Result<Option<RawEvent>>;
    async fn get_discovered(&self, event_id: &str) -> Result<Option<TokenDiscovered>>;
    async fn get_trade(&self, event_id: &str) -> Result<Option<TradeObserved>>;
    async fn get_lifecycle(&self, event_id: &str) -> Result<Option<LifecycleObserved>>;
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    async fn load_checkpoint(&self, ingest_id: &str) -> Result<Option<Checkpoint>>;
    async fn set_persisted_at(&self, event_id: &str, at: DateTime<Utc>) -> Result<()>;
    async fn insert_gap(&self, gap: &IngestGap) -> Result<i64>;
    async fn mark_gap_recovered(&self, id: i64) -> Result<()>;
    async fn upsert_head(&self, head: &ChainHead) -> Result<()>;
    async fn insert_session(&self, session: &CollectionSession) -> Result<i64>;
    async fn finish_session(&self, id: i64, finish: SessionFinish) -> Result<()>;
    async fn get_session(&self, id: i64) -> Result<Option<CollectionSession>>;
    async fn upsert_watched_market(
        &self,
        market: &MarketRef,
        source_event_id: Option<&str>,
    ) -> Result<()>;
    async fn load_watched_markets(&self, chain: Chain) -> Result<Vec<MarketRef>>;
    async fn unrecovered_gap_count(&self, chain: Chain) -> Result<i64>;
    async fn insert_snapshot(&self, snap: &TokenStateSnapshot) -> Result<i64>;
    async fn list_snapshots(
        &self,
        chain: Chain,
        token: &str,
        include_superseded: bool,
    ) -> Result<Vec<TokenStateSnapshot>>;
    async fn latest_snapshot(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenStateSnapshot>>;
    async fn snapshot_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<TokenStateSnapshot>>;
    async fn milestone_snapshot(
        &self,
        chain: Chain,
        token: &str,
        age_ms: i64,
    ) -> Result<Option<TokenStateSnapshot>>;
    async fn mark_snapshots_superseded(&self, chain: Chain, token: &str) -> Result<u64>;
    async fn upsert_current_state(
        &self,
        chain: Chain,
        token: &str,
        snapshot_id: Option<i64>,
        lifecycle: &str,
        last_event_time: Option<DateTime<Utc>>,
        last_event_id: Option<&str>,
        data_quality: QualityStatus,
    ) -> Result<()>;
    async fn load_token_trades(&self, chain: Chain, token: &str) -> Result<Vec<TradeObserved>>;
    async fn load_token_lifecycle(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Vec<LifecycleObserved>>;
    async fn load_token_discovered(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenDiscovered>>;
    async fn insert_assessment(&self, a: &SecurityAssessment) -> Result<i64>;
    async fn list_assessments(&self, chain: Chain, token: &str) -> Result<Vec<SecurityAssessment>>;
    async fn latest_assessment(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<SecurityAssessment>>;
    async fn insert_feature_vector(&self, v: &FeatureVector) -> Result<i64>;
    async fn list_feature_vectors(&self, chain: Chain, token: &str) -> Result<Vec<FeatureVector>>;
    async fn feature_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<FeatureVector>>;
    async fn insert_candidate_transition(&self, t: &CandidateTransition) -> Result<i64>;
    async fn list_candidate_transitions(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Vec<CandidateTransition>>;
    async fn latest_candidate(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Option<CandidateTransition>>;
    async fn export_feature_vectors(
        &self,
        chain: Option<Chain>,
        limit: i64,
    ) -> Result<Vec<FeatureVector>>;
}

#[derive(Debug, Clone)]
pub struct SessionFinish {
    pub ended_at: DateTime<Utc>,
    pub end_block: Option<i64>,
    pub end_slot: Option<i64>,
    pub complete: bool,
    pub quality_status: QualityStatus,
    pub gap_count: i32,
    pub notes: Option<String>,
}

pub fn apply_orphan(event: &mut RawEvent) {
    event.canonical_status = CanonicalStatus::Orphaned;
    if let crate::domain::raw_event::RawEventKind::Evm(log) = &mut event.kind {
        log.removed = true;
    }
}

pub fn with_finality(event: &mut RawEvent, finality: Finality) {
    event.finality = finality;
    if let crate::domain::raw_event::RawEventKind::Solana(ix) = &mut event.kind {
        ix.finality = finality;
    }
}
