use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::candidate::CandidateTransition;
use crate::domain::{
    Chain, CollectionSession, DecoderStatus, LifecycleObserved, QualityStatus, RawEvent,
    TokenDiscovered, TradeObserved,
};
use crate::error::{EngineError, Result};
use crate::features::FeatureVector;
use crate::lab::experiment::StrategyExperiment;
use crate::lab::persist::SimStore;
use crate::security::SecurityAssessment;
use crate::sim::harness::SimulationReport;
use crate::sim::outcome::TokenOutcome;
use crate::sim::types::SimulationRun;
use crate::sim::PolicyPerformance;
use crate::state::TokenStateSnapshot;
use crate::watch::MarketRef;

use super::{ChainHead, Checkpoint, EventStore, IngestGap, InsertRaw, SessionFinish};

#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    raw: HashMap<String, RawEvent>,
    discovered: HashMap<String, TokenDiscovered>,
    trades: HashMap<String, TradeObserved>,
    lifecycle: HashMap<String, LifecycleObserved>,
    checkpoints: HashMap<String, Checkpoint>,
    gaps: Vec<IngestGap>,
    heads: HashMap<String, ChainHead>,
    next_gap: i64,
    sessions: HashMap<i64, CollectionSession>,
    next_session: i64,
    markets: Vec<MarketRef>,
    snapshots: Vec<TokenStateSnapshot>,
    next_snapshot: i64,
    assessments: Vec<SecurityAssessment>,
    next_assessment: i64,
    features: Vec<FeatureVector>,
    next_feature: i64,
    candidates: Vec<CandidateTransition>,
    next_candidate: i64,
    sim_runs: Vec<SimulationRun>,
    sim_reports: std::collections::HashMap<i64, SimulationReport>,
    next_sim_run: i64,
    outcomes: Vec<TokenOutcome>,
    next_outcome: i64,
    experiments: std::collections::HashMap<String, StrategyExperiment>,
    perf: Vec<(i64, PolicyPerformance)>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trades(&self) -> Vec<TradeObserved> {
        self.inner
            .lock()
            .unwrap()
            .trades
            .values()
            .cloned()
            .collect()
    }

    pub fn lifecycle(&self) -> Vec<LifecycleObserved> {
        self.inner
            .lock()
            .unwrap()
            .lifecycle
            .values()
            .cloned()
            .collect()
    }

    pub fn gaps(&self) -> Vec<IngestGap> {
        self.inner.lock().unwrap().gaps.clone()
    }

    pub fn sessions(&self) -> Vec<CollectionSession> {
        self.inner
            .lock()
            .unwrap()
            .sessions
            .values()
            .cloned()
            .collect()
    }

    pub fn raw_count(&self) -> usize {
        self.inner.lock().unwrap().raw.len()
    }

    pub fn feature_vectors(&self) -> Vec<FeatureVector> {
        self.inner.lock().unwrap().features.clone()
    }

    pub fn snapshots(&self) -> Vec<TokenStateSnapshot> {
        self.inner.lock().unwrap().snapshots.clone()
    }

    pub fn candidates(&self) -> Vec<CandidateTransition> {
        self.inner.lock().unwrap().candidates.clone()
    }

    pub fn discovered(&self) -> Vec<TokenDiscovered> {
        self.inner
            .lock()
            .unwrap()
            .discovered
            .values()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl EventStore for MemoryStore {
    async fn insert_raw(&self, event: &RawEvent) -> Result<InsertRaw> {
        let mut inner = self.inner.lock().unwrap();
        let id = event.event_id();
        if inner.raw.contains_key(&id) {
            return Ok(InsertRaw::Duplicate);
        }
        let mut stored = event.clone();
        stored.persisted_at = Some(Utc::now());
        inner.raw.insert(id, stored);
        Ok(InsertRaw::Inserted)
    }

    async fn insert_discovered(&self, token: &TokenDiscovered) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .discovered
            .insert(token.raw_event_id.clone(), token.clone());
        Ok(())
    }

    async fn insert_trade(&self, trade: &TradeObserved) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.trades.insert(trade.event_id.clone(), trade.clone());
        Ok(())
    }

    async fn insert_lifecycle(&self, life: &LifecycleObserved) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.lifecycle.insert(life.event_id.clone(), life.clone());
        Ok(())
    }

    async fn mark_decoder(
        &self,
        event_id: &str,
        status: DecoderStatus,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let event = inner
            .raw
            .get_mut(event_id)
            .ok_or_else(|| EngineError::Storage(format!("missing raw {event_id}")))?;
        event.decoder_status = status;
        event.decoder_version = version.map(|s| s.to_string());
        event.error = error.map(|s| s.to_string());
        Ok(())
    }

    async fn mark_orphaned(&self, event_id: &str) -> Result<bool> {
        let mut inner = self.inner.lock().unwrap();
        match inner.raw.get_mut(event_id) {
            Some(event) => {
                super::apply_orphan(event);
                if let Some(t) = inner.trades.get_mut(event_id) {
                    t.canonical_status = crate::domain::CanonicalStatus::Orphaned;
                }
                if let Some(l) = inner.lifecycle.get_mut(event_id) {
                    l.canonical_status = crate::domain::CanonicalStatus::Orphaned;
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn get_raw(&self, event_id: &str) -> Result<Option<RawEvent>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.raw.get(event_id).cloned())
    }

    async fn get_discovered(&self, event_id: &str) -> Result<Option<TokenDiscovered>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.discovered.get(event_id).cloned())
    }

    async fn get_trade(&self, event_id: &str) -> Result<Option<TradeObserved>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.trades.get(event_id).cloned())
    }

    async fn get_lifecycle(&self, event_id: &str) -> Result<Option<LifecycleObserved>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.lifecycle.get(event_id).cloned())
    }

    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .checkpoints
            .insert(checkpoint.ingest_id.clone(), checkpoint.clone());
        Ok(())
    }

    async fn load_checkpoint(&self, ingest_id: &str) -> Result<Option<Checkpoint>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.checkpoints.get(ingest_id).cloned())
    }

    async fn set_persisted_at(&self, event_id: &str, at: DateTime<Utc>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(event) = inner.raw.get_mut(event_id) {
            event.persisted_at = Some(at);
        }
        if let Some(td) = inner.discovered.get_mut(event_id) {
            td.persisted_at = Some(at);
        }
        if let Some(t) = inner.trades.get_mut(event_id) {
            t.persisted_at = Some(at);
        }
        if let Some(l) = inner.lifecycle.get_mut(event_id) {
            l.persisted_at = Some(at);
        }
        Ok(())
    }

    async fn insert_gap(&self, gap: &IngestGap) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_gap += 1;
        let id = inner.next_gap;
        let mut g = gap.clone();
        g.id = Some(id);
        inner.gaps.push(g);
        Ok(id)
    }

    async fn mark_gap_recovered(&self, id: i64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(g) = inner.gaps.iter_mut().find(|g| g.id == Some(id)) {
            g.recovered = true;
            g.recovered_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn upsert_head(&self, head: &ChainHead) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .heads
            .insert(head.chain.as_str().to_string(), head.clone());
        Ok(())
    }

    async fn insert_session(&self, session: &CollectionSession) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_session += 1;
        let id = inner.next_session;
        let mut stored = session.clone();
        stored.id = Some(id);
        inner.sessions.insert(id, stored);
        Ok(id)
    }

    async fn finish_session(&self, id: i64, finish: SessionFinish) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(s) = inner.sessions.get_mut(&id) {
            s.ended_at = Some(finish.ended_at);
            s.end_block = finish.end_block;
            s.end_slot = finish.end_slot;
            s.complete = finish.complete;
            s.quality_status = finish.quality_status;
            s.gap_count = finish.gap_count;
            s.notes = finish.notes;
        }
        Ok(())
    }

    async fn get_session(&self, id: i64) -> Result<Option<CollectionSession>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.sessions.get(&id).cloned())
    }

    async fn upsert_watched_market(
        &self,
        market: &MarketRef,
        _source_event_id: Option<&str>,
    ) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(existing) = inner
            .markets
            .iter_mut()
            .find(|m| m.chain == market.chain && m.token_address == market.token_address)
        {
            *existing = market.clone();
        } else {
            inner.markets.push(market.clone());
        }
        Ok(())
    }

    async fn load_watched_markets(&self, chain: crate::domain::Chain) -> Result<Vec<MarketRef>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .markets
            .iter()
            .filter(|m| m.chain == chain)
            .cloned()
            .collect())
    }

    async fn unrecovered_gap_count(&self, chain: crate::domain::Chain) -> Result<i64> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .gaps
            .iter()
            .filter(|g| g.chain == chain && !g.recovered)
            .count() as i64)
    }

    async fn insert_snapshot(&self, snap: &TokenStateSnapshot) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_snapshot += 1;
        let id = inner.next_snapshot;
        let mut stored = snap.clone();
        stored.id = Some(id);
        inner.snapshots.push(stored);
        Ok(id)
    }

    async fn list_snapshots(
        &self,
        chain: Chain,
        token: &str,
        include_superseded: bool,
    ) -> Result<Vec<TokenStateSnapshot>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .snapshots
            .iter()
            .filter(|s| {
                s.chain == chain
                    && s.token_address == token
                    && (include_superseded || !s.superseded)
            })
            .cloned()
            .collect())
    }

    async fn latest_snapshot(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenStateSnapshot>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .snapshots
            .iter()
            .filter(|s| s.chain == chain && s.token_address == token && !s.superseded)
            .max_by_key(|s| (s.snapshot_time, s.version, s.id.unwrap_or(0)))
            .cloned())
    }

    async fn snapshot_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<TokenStateSnapshot>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .snapshots
            .iter()
            .filter(|s| {
                s.chain == chain
                    && s.token_address == token
                    && !s.superseded
                    && s.snapshot_time <= time
            })
            .max_by_key(|s| (s.snapshot_time, s.version))
            .cloned())
    }

    async fn milestone_snapshot(
        &self,
        chain: Chain,
        token: &str,
        age_ms: i64,
    ) -> Result<Option<TokenStateSnapshot>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .snapshots
            .iter()
            .filter(|s| {
                s.chain == chain
                    && s.token_address == token
                    && !s.superseded
                    && s.snapshot_kind == crate::state::SnapshotKind::Milestone
                    && s.age_ms == age_ms
            })
            .max_by_key(|s| s.version)
            .cloned())
    }

    async fn mark_snapshots_superseded(&self, chain: Chain, token: &str) -> Result<u64> {
        let mut inner = self.inner.lock().unwrap();
        let mut n = 0u64;
        for s in &mut inner.snapshots {
            if s.chain == chain && s.token_address == token && !s.superseded {
                s.superseded = true;
                n += 1;
            }
        }
        Ok(n)
    }

    async fn upsert_current_state(
        &self,
        _chain: Chain,
        _token: &str,
        _snapshot_id: Option<i64>,
        _lifecycle: &str,
        _last_event_time: Option<DateTime<Utc>>,
        _last_event_id: Option<&str>,
        _data_quality: QualityStatus,
    ) -> Result<()> {
        Ok(())
    }

    async fn load_token_trades(&self, chain: Chain, token: &str) -> Result<Vec<TradeObserved>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .trades
            .values()
            .filter(|t| t.chain == chain && t.token_address == token)
            .cloned()
            .collect())
    }

    async fn load_token_lifecycle(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Vec<LifecycleObserved>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .lifecycle
            .values()
            .filter(|t| t.chain == chain && t.token_address == token)
            .cloned()
            .collect())
    }

    async fn load_token_discovered(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenDiscovered>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .discovered
            .values()
            .find(|t| t.chain == chain && t.token_address == token)
            .cloned())
    }

    async fn insert_assessment(&self, a: &SecurityAssessment) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_assessment += 1;
        let id = inner.next_assessment;
        let mut stored = a.clone();
        stored.id = Some(id);
        inner.assessments.push(stored);
        Ok(id)
    }

    async fn list_assessments(&self, chain: Chain, token: &str) -> Result<Vec<SecurityAssessment>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .assessments
            .iter()
            .filter(|a| a.chain == chain && a.token_address == token)
            .cloned()
            .collect())
    }

    async fn latest_assessment(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<SecurityAssessment>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .assessments
            .iter()
            .filter(|a| a.chain == chain && a.token_address == token)
            .max_by_key(|a| a.id.unwrap_or(0))
            .cloned())
    }

    async fn insert_feature_vector(&self, v: &FeatureVector) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_feature += 1;
        let id = inner.next_feature;
        let mut stored = v.clone();
        stored.id = Some(id);
        inner.features.push(stored);
        Ok(id)
    }

    async fn list_feature_vectors(&self, chain: Chain, token: &str) -> Result<Vec<FeatureVector>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .features
            .iter()
            .filter(|f| f.chain == chain && f.token_address == token)
            .cloned()
            .collect())
    }

    async fn feature_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<FeatureVector>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .features
            .iter()
            .filter(|f| f.chain == chain && f.token_address == token && f.as_of_time <= time)
            .max_by_key(|f| f.as_of_time)
            .cloned())
    }

    async fn insert_candidate_transition(&self, t: &CandidateTransition) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_candidate += 1;
        let id = inner.next_candidate;
        let mut stored = t.clone();
        stored.id = Some(id);
        inner.candidates.push(stored);
        Ok(id)
    }

    async fn list_candidate_transitions(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Vec<CandidateTransition>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .candidates
            .iter()
            .filter(|c| c.chain == chain && c.token_address == token && c.policy_id == policy_id)
            .cloned()
            .collect())
    }

    async fn latest_candidate(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Option<CandidateTransition>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .candidates
            .iter()
            .filter(|c| c.chain == chain && c.token_address == token && c.policy_id == policy_id)
            .max_by_key(|c| c.id.unwrap_or(0))
            .cloned())
    }

    async fn export_feature_vectors(
        &self,
        chain: Option<Chain>,
        limit: i64,
    ) -> Result<Vec<FeatureVector>> {
        let inner = self.inner.lock().unwrap();
        let mut rows: Vec<FeatureVector> = inner
            .features
            .iter()
            .filter(|f| chain.map(|c| f.chain == c).unwrap_or(true))
            .cloned()
            .collect();
        rows.sort_by_key(|f| f.as_of_time);
        if limit > 0 {
            rows.truncate(limit as usize);
        }
        Ok(rows)
    }
}

#[async_trait]
impl SimStore for MemoryStore {
    async fn insert_simulation_run(&self, r: &SimulationRun) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_sim_run += 1;
        let id = inner.next_sim_run;
        let mut stored = r.clone();
        stored.id = Some(id);
        inner.sim_runs.push(stored);
        Ok(id)
    }

    async fn get_simulation_run(&self, id: i64) -> Result<Option<SimulationRun>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.sim_runs.iter().find(|r| r.id == Some(id)).cloned())
    }

    async fn persist_report(&self, report: &SimulationReport) -> Result<i64> {
        let id = self.insert_simulation_run(&report.run).await?;
        let mut stored = report.clone();
        stored.run.id = Some(id);
        self.inner.lock().unwrap().sim_reports.insert(id, stored);
        Ok(id)
    }

    async fn load_report(&self, run_id: i64) -> Result<Option<SimulationReport>> {
        Ok(self.inner.lock().unwrap().sim_reports.get(&run_id).cloned())
    }

    async fn insert_token_outcome(&self, o: &TokenOutcome) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_outcome += 1;
        let id = inner.next_outcome;
        inner.outcomes.push(o.clone());
        Ok(id)
    }

    async fn insert_policy_performance(&self, run_id: i64, p: &PolicyPerformance) -> Result<i64> {
        let mut inner = self.inner.lock().unwrap();
        inner.perf.push((run_id, p.clone()));
        Ok(inner.perf.len() as i64)
    }

    async fn upsert_experiment(&self, e: &StrategyExperiment) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .experiments
            .insert(e.experiment_id.clone(), e.clone());
        Ok(())
    }

    async fn get_experiment(&self, id: &str) -> Result<Option<StrategyExperiment>> {
        Ok(self.inner.lock().unwrap().experiments.get(id).cloned())
    }
}
