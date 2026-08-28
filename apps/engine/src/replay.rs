use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::mpsc;

use crate::decoders::DecoderRegistry;
use crate::domain::{
    CollectionSession, LifecycleObserved, QualityStatus, SolanaMode, TokenDiscovered, TradeObserved,
};
use crate::error::Result;
use crate::historical::{FixtureSource, HistoricalSource, PumpCorpusSource};
use crate::metrics::DiscoveryMetrics;
use crate::pipeline::{DiscoveryPipeline, HandleResult};
use crate::state::{StateEngine, TokenStateSnapshot};
use crate::storage::EventStore;
use crate::watch::MarketRegistry;

#[derive(Debug, Clone)]
pub struct ReplayReport {
    pub raw_handled: usize,
    pub duplicates: usize,
    pub unknown: usize,
    pub tokens: Vec<TokenDiscovered>,
    pub trades: Vec<TradeObserved>,
    pub lifecycle: Vec<LifecycleObserved>,
    pub duration: std::time::Duration,
    pub session: CollectionSession,
    pub snapshots: Vec<TokenStateSnapshot>,
    pub feature_vectors: Vec<crate::features::FeatureVector>,
    pub candidate_transitions: Vec<crate::candidate::CandidateTransition>,
}

impl ReplayReport {
    pub fn canonical_fingerprint(&self) -> String {
        let mut ids: Vec<String> = self
            .tokens
            .iter()
            .map(|t| format!("td:{}", t.raw_event_id))
            .chain(self.trades.iter().map(|t| format!("tr:{}", t.event_id)))
            .chain(self.lifecycle.iter().map(|l| format!("lf:{}", l.event_id)))
            .collect();
        ids.sort();
        ids.join("\n")
    }

    pub fn snapshot_fingerprint(&self) -> String {
        let mut hs: Vec<String> = self
            .snapshots
            .iter()
            .map(|s| s.fingerprint.clone())
            .collect();
        hs.sort();
        hs.join("\n")
    }
}

/// Replay a historical source through the same [`DecoderRegistry`] used live.
pub async fn replay_source<S, H>(
    source: &mut H,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    ingest_id: &str,
    provider: &str,
    snapshots: bool,
) -> Result<ReplayReport>
where
    S: EventStore + 'static,
    H: HistoricalSource,
{
    replay_source_with(
        source, store, markets, ingest_id, provider, snapshots, false,
    )
    .await
}

#[derive(Debug, Clone)]
pub struct ReplayOpts {
    pub snapshots: bool,
    pub features: bool,
    pub quality: QualityStatus,
    pub complete: bool,
    pub notes: Option<String>,
}

impl ReplayOpts {
    pub fn fixture(snapshots: bool, features: bool) -> Self {
        Self {
            snapshots,
            features,
            quality: QualityStatus::HistoricalReplay,
            complete: true,
            notes: Some("offline fixture/historical replay".into()),
        }
    }

    pub fn corpus(quality: QualityStatus, complete: bool) -> Self {
        Self {
            snapshots: false,
            features: false,
            quality,
            complete,
            notes: Some("decoded research corpus replay".into()),
        }
    }
}

pub async fn replay_source_with<S, H>(
    source: &mut H,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    ingest_id: &str,
    provider: &str,
    snapshots: bool,
    features: bool,
) -> Result<ReplayReport>
where
    S: EventStore + 'static,
    H: HistoricalSource,
{
    replay_source_opts(
        source,
        store,
        markets,
        ingest_id,
        provider,
        ReplayOpts::fixture(snapshots, features),
    )
    .await
}

pub async fn replay_source_opts<S, H>(
    source: &mut H,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    ingest_id: &str,
    provider: &str,
    opts: ReplayOpts,
) -> Result<ReplayReport>
where
    S: EventStore + 'static,
    H: HistoricalSource,
{
    let snapshots = opts.snapshots;
    let features = opts.features;
    let started = Instant::now();
    let (discovered_tx, mut discovered_rx) = mpsc::channel(1024);
    let (trade_tx, mut trade_rx) = mpsc::channel(4096);
    let (life_tx, mut life_rx) = mpsc::channel(1024);
    let mut session = CollectionSession::start(
        crate::domain::Chain::Solana,
        SolanaMode::Historical,
        provider,
        opts.notes.clone(),
    );
    session.complete = opts.complete;
    session.quality_status = opts.quality;
    let session_id = store.insert_session(&session).await?;
    session.id = Some(session_id);
    let want_snapshots = snapshots || features;
    let state = if want_snapshots {
        Some(Arc::new(Mutex::new(StateEngine::replay(
            opts.quality,
            Some(session_id),
        ))))
    } else {
        None
    };
    let pipeline = DiscoveryPipeline {
        store: store.clone(),
        registry: DecoderRegistry::production(),
        markets,
        discovered_tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: DiscoveryMetrics,
        ingest_id: ingest_id.into(),
        slots: None,
        pool_tx: None,
        state: state.clone(),
    };

    let mut raw_handled = 0usize;
    let mut duplicates = 0usize;
    let mut unknown = 0usize;
    let mut start_slot = None;
    let mut end_slot = None;

    while let Some(raw) = source.next_event().await? {
        if let Some(slot) = raw.slot() {
            start_slot = Some(start_slot.map_or(slot, |s: i64| s.min(slot)));
            end_slot = Some(end_slot.map_or(slot, |s: i64| s.max(slot)));
        }
        DiscoveryMetrics::historical_replay_event();
        match pipeline.handle(raw).await? {
            HandleResult::Duplicate { .. } => duplicates += 1,
            HandleResult::Unknown { .. } => unknown += 1,
            HandleResult::DecodeError { .. } | HandleResult::Orphaned { .. } => unknown += 1,
            HandleResult::Discovered(_) | HandleResult::Canonical { .. } => {}
        }
        raw_handled += 1;
    }

    let mut out_snaps = Vec::new();
    if let Some(eng) = &state {
        let extra = {
            let mut g = eng.lock().expect("state");
            g.finish_all_milestones()
        };
        for snap in extra {
            let _ = store.insert_snapshot(&snap).await;
        }
        out_snaps = eng.lock().expect("state").history.clone();
    }

    drop(pipeline);
    let mut tokens = Vec::new();
    let mut trades = Vec::new();
    let mut lifecycle = Vec::new();
    while let Ok(t) = discovered_rx.try_recv() {
        tokens.push(t);
    }
    while let Ok(t) = trade_rx.try_recv() {
        trades.push(t);
    }
    while let Ok(l) = life_rx.try_recv() {
        lifecycle.push(l);
    }

    let duration = started.elapsed();
    DiscoveryMetrics::historical_replay_duration(duration);
    session.start_slot = start_slot;
    session.end_slot = end_slot;
    session.ended_at = Some(chrono::Utc::now());
    store
        .finish_session(
            session_id,
            crate::storage::SessionFinish {
                ended_at: session.ended_at.unwrap(),
                end_block: None,
                end_slot,
                complete: opts.complete,
                quality_status: opts.quality,
                gap_count: 0,
                notes: session.notes.clone(),
            },
        )
        .await?;

    let mut feature_vectors = Vec::new();
    let mut candidate_transitions = Vec::new();
    if features {
        let assessments: Vec<_> = tokens
            .iter()
            .map(|tok| {
                let ctx = crate::security::context::SecurityContext::from_token(
                    tok.clone(),
                    opts.quality,
                    true,
                );
                crate::security::SecurityEngine::default().assess(&ctx)
            })
            .collect();
        let batch = crate::features::process_snapshots(
            &out_snaps,
            &assessments,
            &crate::candidate::CandidateEngine::default_research(),
        );
        for v in &batch.vectors {
            let _ = store.insert_feature_vector(v).await;
        }
        for t in &batch.transitions {
            let _ = store.insert_candidate_transition(t).await;
        }
        feature_vectors = batch.vectors;
        candidate_transitions = batch.transitions;
    }

    Ok(ReplayReport {
        raw_handled,
        duplicates,
        unknown,
        tokens,
        trades,
        lifecycle,
        duration,
        session,
        snapshots: out_snaps,
        feature_vectors,
        candidate_transitions,
    })
}

pub async fn replay_fixture_dir<S: EventStore + 'static>(
    dir: &Path,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
) -> Result<ReplayReport> {
    replay_fixture_dir_opts(dir, store, markets, false).await
}

pub async fn replay_fixture_dir_opts<S: EventStore + 'static>(
    dir: &Path,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    snapshots: bool,
) -> Result<ReplayReport> {
    replay_fixture_dir_full(dir, store, markets, snapshots, false).await
}

pub async fn replay_corpus_jsonl<S: EventStore + 'static>(
    path: &Path,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    opts: ReplayOpts,
) -> Result<ReplayReport> {
    let mut source = PumpCorpusSource::open(path)?;
    replay_source_opts(
        &mut source,
        store,
        markets,
        "solana:historical:corpus",
        "Slinky21/Pumpfun_Memecoin_Corpus",
        opts,
    )
    .await
}

pub async fn replay_fixture_dir_full<S: EventStore + 'static>(
    dir: &Path,
    store: Arc<S>,
    markets: Arc<MarketRegistry>,
    snapshots: bool,
    features: bool,
) -> Result<ReplayReport> {
    let mut source = FixtureSource::from_dir(dir)?;
    replay_source_with(
        &mut source,
        store,
        markets,
        "solana:historical",
        &format!("fixture:{}", dir.display()),
        snapshots,
        features,
    )
    .await
}

pub fn format_report(report: &ReplayReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "historical replay  quality={}  complete={}  raw={}  duplicates={}  unknown={}  snapshots={}  features={}  candidates={}  {:.3}s\n",
        report.session.quality_status.as_str(),
        report.session.complete,
        report.raw_handled,
        report.duplicates,
        report.unknown,
        report.snapshots.len(),
        report.feature_vectors.len(),
        report.candidate_transitions.len(),
        report.duration.as_secs_f64()
    ));
    for t in &report.tokens {
        out.push_str(&format!(
            "  TokenDiscovered {} curve={:?} pool={:?}\n",
            t.token_address, t.curve, t.pool
        ));
    }
    for t in &report.trades {
        out.push_str(&format!(
            "  TradeObserved {} {} launchpad={}\n",
            t.side.as_str(),
            t.token_address,
            t.launchpad.as_str()
        ));
    }
    for l in &report.lifecycle {
        out.push_str(&format!(
            "  LifecycleObserved {} token={} curve={:?} pool={:?}\n",
            l.lifecycle_type.as_str(),
            l.token_address,
            l.curve,
            l.pool
        ));
    }
    for s in report.snapshots.iter().take(24) {
        out.push_str(&format!(
            "  snapshot {} age={}ms life={} buys={} sells={} q={}\n",
            s.snapshot_kind.as_str(),
            s.age_ms,
            s.lifecycle_state.as_str(),
            s.buy_count_total,
            s.sell_count_total,
            s.data_quality.as_str()
        ));
    }
    out
}
